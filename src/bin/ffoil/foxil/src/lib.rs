use crate::result::XfoilResult;

use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::vec::Vec;

pub mod error;
pub mod result;

pub enum Mode {
    Angle(f64),
    AngleBatch(Vec<f64>),
    AngleRange(f64, f64, f64),
    Cl(f64),
}

/// Struct tracking Xfoil configuration.
pub struct FoxConfig {
    mode: Mode,
    reynolds: Option<usize>,
    path: PathBuf,
    polar: Option<PathBuf>,
    naca: Option<String>,
    dat_file: Option<PathBuf>,
}

impl FoxConfig {
    /// Create new Xfoil configuration structure from the path to an Xfoil executable.
    pub fn new<T: AsRef<Path>>(path: T) -> Self {
        Self {
            mode: Mode::Angle(0.0),
            reynolds: None,
            path: path.as_ref().to_path_buf(),
            polar: None,
            naca: None,
            dat_file: None,
        }
    }

    /// Construct XfoilRunner from configuration
    /// panics: if no airfoil (either from polar file or NACA code) is given.
    pub fn get_runner(mut self) -> error::Result<XfoilRunner> {
        let mut command_sequence = vec!["plop", "G", ""]
            .into_iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>();

        if let Some(naca) = self.naca {
            command_sequence.push(format!("naca {naca}").to_string());
        } else if let Some(dat) = self.dat_file {
            command_sequence.extend_from_slice(&[
                format!("load {}", dat.display()).to_string(),
                "".to_string(),
            ]);
        } else {
            panic!("Xfoil cannot run without airfoil");
        }

        command_sequence.push("oper".to_string());

        if let Some(reynolds) = self.reynolds {
            command_sequence.push(format!("v {reynolds}").to_string());
        }

        self.polar = if let Some(polar) = self.polar {
            command_sequence.extend_from_slice(&[
                "pacc".to_string(),
                polar.to_string_lossy().to_string(),
                "".to_string(),
            ]);
            Some(polar)
        } else {
            None
        };

        match self.mode {
            Mode::Angle(angle) => {
                command_sequence.extend_from_slice(&[format!("a {angle}").to_string()])
            }
            Mode::Cl(cl) => command_sequence.extend_from_slice(&[format!("cl {cl}").to_string()]),
            Mode::AngleBatch(angles) => {
                command_sequence.extend(angles.iter().map(|angle| format!("a {angle}")))
            }
            Mode::AngleRange(start, end, step) => command_sequence
                .extend_from_slice(&[format!("aseq {start} {end} {step}").to_string()]),
        }

        command_sequence.push("".to_string());
        command_sequence.push("quit".to_string());
        Ok(XfoilRunner {
            xfoil_path: self.path,
            command_sequence,
            polar: self.polar,
        })
    }

    pub fn mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }
    /// Set angle of attack at which to run xfoil computation.
    /// If lift_coefficient was previously called, the state is
    /// overwritten to use an angle of attack calculation instead.
    pub fn aoa(self, angle: f64) -> Self {
        self.mode(Mode::Angle(angle))
    }

    pub fn aoa_batch(self, angles: Vec<f64>) -> Self {
        self.mode(Mode::AngleBatch(angles))
    }

    pub fn aoa_range(self, start: f64, end: f64, step: f64) -> Self {
        self.mode(Mode::AngleRange(start, end, step))
    }

    /// Set lift coefficient at which to run xfoil computation.
    /// If angle_of_attack was previously called, the state is
    /// overwritten to use a lift coefficient calculation instead.
    pub fn lift_coefficient(mut self, cl: f64) -> Self {
        self.mode = Mode::Cl(cl);
        self
    }

    /// Set path of polar file to save Xfoil data into.
    pub fn polar_accumulation<T: AsRef<Path>>(mut self, fname: T) -> Self {
        let buf = fname.as_ref().to_path_buf();
        fs::create_dir_all(buf.parent().unwrap()).expect("Failed to create parent directory");
        if buf.exists() {
            fs::remove_file(&buf).expect("Error deleting existing file");
        }

        self.polar = Some(buf);
        self
    }

    /// Specify a 4-digit NACA airfoil code.
    pub fn naca(mut self, code: &str) -> Self {
        self.naca = Some(code.to_string());
        self.dat_file = None;
        self
    }

    /// Specify a file containing airfoil coordinates to use in Xfoil computation.
    pub fn airfoil_polar_file<T: AsRef<Path>>(mut self, path: T) -> Self {
        self.dat_file = Some(path.as_ref().to_path_buf());
        self.naca = None;
        self
    }

    /// Set a Reynolds number for a viscous calculation.
    pub fn reynolds(mut self, reynolds: usize) -> Self {
        self.reynolds = Some(reynolds);
        self
    }
}

pub struct XfoilRunner {
    xfoil_path: PathBuf,
    command_sequence: Vec<String>,
    polar: Option<PathBuf>,
}

impl XfoilRunner {
    /// Run Xfoil calculation. This method dispatches a child process, and feeds
    /// a sequence of commands to its stdin. After the calculation finishes,
    /// it outputs the contents of the resulting polar file in a HashMap.
    /// This method panics if something goes wrong either executing the child
    /// process, or retrieving a handle to its stdin. It may return an XfoilError
    /// if anything goes wrong writing to the process or parsing its output.
    pub fn dispatch(self) -> error::Result<XfoilResult> {
        let mut child = Command::new(&self.xfoil_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to execute Xfoil");

        let stdin = child
            .stdin
            .as_mut()
            .expect("Failed to retrieve handle to child stdin");

        let write_result = (|| {
            stdin.write_all(self.command_sequence.join("\n").as_bytes())?;
            Ok(())
        })();

        if let Err(e) = write_result {
            // Wait on the child to prevent zombie process
            let _ = child.wait()?; // ignore error, we're only concerned with reaping the process
            return Err(e);
        }

        // If the calculation did not convergence, return ConvergenceError
        let _ = child
            .wait_with_output()
            .expect("Failed to retrieve child output");

        self.polar
            .as_ref()
            .map(|p| self.parse_polar(p))
            .expect("The polar file is not specified!")
    }

    fn parse_polar<T: AsRef<Path>>(&self, path: T) -> error::Result<XfoilResult> {
        let mut result = HashMap::new();
        let table_header = ["alpha", "CL", "CD", "CDp", "CM", "Top_Xtr", "Bot_Xtr"];
        for header in &table_header {
            result.insert(header.to_string(), Vec::<f64>::new());
        }
        // number of lines in Xfoil polar header
        const HEADER: usize = 13;
        for line in BufReader::new(File::open(path)?).lines().skip(HEADER - 1) {
            let data = line?
                .split_whitespace()
                .map(|x| x.parse::<f64>().expect("Failed to parse Xfoil polar"))
                .collect::<Vec<_>>();
            for (&header, value) in table_header.iter().zip(data) {
                result
                    .get_mut(header)
                    .expect("Failed to retrieve result HashMap")
                    .push(value);
            }
        }
        Ok(
            serde_json::from_value(serde_json::json!(result))
                .expect("Failed to deserialize result"),
        )
    }
}
