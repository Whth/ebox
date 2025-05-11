use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::vec::Vec;
use tempfile::tempdir;

pub mod error;

enum Mode {
    Angle(f64),
    Cl(f64),
}

/// Struct tracking Xfoil configuration.
pub struct Config {
    mode: Mode,
    reynolds: Option<usize>,
    path: String,
    polar: Option<String>,
    naca: Option<String>,
    dat_file: Option<String>,
}

impl Config {
    /// Create new Xfoil configuration structure from the path to an Xfoil executable.
    pub fn new(path: &str) -> Self {
        Self {
            mode: Mode::Angle(0.0),
            reynolds: None,
            path: path.to_string(),
            polar: None,
            naca: None,
            dat_file: None,
        }
    }

    /// Construct XfoilRunner from configuration
    /// panics: if no airfoil (either from polar file or NACA code) is given.
    pub fn get_runner(mut self) -> error::Result<XfoilRunner> {
        let mut command_sequence = vec!["plop", "G", "\n"]
            .into_iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>();

        if let Some(naca) = self.naca {
            command_sequence.push(format!("naca {}", naca).to_string());
        } else if let Some(dat) = self.dat_file {
            command_sequence
                .extend_from_slice(&[format!("load {}", dat).to_string(), "".to_string()]);
        } else {
            panic!("Xfoil cannot run without airfoil");
        }

        if let Some(reynolds) = self.reynolds {
            command_sequence.extend_from_slice(&[
                "oper".to_string(),
                format!("v {}", reynolds).to_string(),
                "\n".to_string(),
            ]);
        }

        self.polar = if let Some(polar) = self.polar {
            command_sequence.extend_from_slice(&[
                "oper".to_string(),
                "pacc".to_string(),
                polar.to_string(),
                "\n".to_string(),
            ]);
            Some(polar)
        } else {
            None
        };

        match self.mode {
            Mode::Angle(angle) => command_sequence.extend_from_slice(&[
                "oper".to_string(),
                format!("a {}", angle).to_string(),
                "\n".to_string(),
            ]),
            Mode::Cl(cl) => command_sequence.extend_from_slice(&[
                "oper".to_string(),
                format!("cl {}", cl).to_string(),
                "\n".to_string(),
            ]),
        }

        command_sequence.push("quit".to_string());

        Ok(XfoilRunner {
            xfoil_path: self.path,
            command_sequence,
            polar: self.polar,
        })
    }

    /// Set angle of attack at which to run xfoil computation.
    /// If lift_coefficient was previously called, the state is
    /// overwritten to use an angle of attack calculation instead.
    pub fn angle_of_attack(mut self, angle: f64) -> Self {
        self.mode = Mode::Angle(angle);
        self
    }

    /// Set lift coefficient at which to run xfoil computation.
    /// If angle_of_attack was previously called, the state is
    /// overwritten to use a lift coefficient calculation instead.
    pub fn lift_coefficient(mut self, cl: f64) -> Self {
        self.mode = Mode::Cl(cl);
        self
    }

    /// Set path of polar file to save Xfoil data into.
    pub fn pacc_from_str(mut self, fname: &str) -> Self {
        self.polar = Some(fname.to_string());
        self
    }

    pub fn pacc_random(mut self) -> Self {
        self.polar = Some(
            tempdir()
                .expect("Failed to create tempdir")
                .path()
                .join("polar.dat")
                .to_string_lossy()
                .to_string(),
        );
        self
    }

    /// Specify a 4-digit NACA airfoil code.
    pub fn naca(mut self, code: &str) -> Self {
        self.naca = Some(code.to_string());
        self.dat_file = None;
        self
    }

    /// Specify a file containing airfoil coordinates to use in Xfoil computation.
    pub fn airfoil_polar_file(mut self, path: &str) -> Self {
        self.dat_file = Some(path.to_string());
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
    xfoil_path: String,
    command_sequence: Vec<String>,
    polar: Option<String>,
}

impl XfoilRunner {
    /// Run Xfoil calculation. This method dispatches a child process, and feeds
    /// a sequence of commands to its stdin. After the calculation finishes,
    /// it outputs the contents of the resulting polar file in a HashMap.
    /// This method panics if something goes wrong either executing the child
    /// process, or retrieving a handle to its stdin. It may return an XfoilError
    /// if anything goes wrong writing to the process or parsing its output.
    pub fn dispatch(self) -> error::Result<HashMap<String, Vec<f64>>> {
        let mut child = Command::new(&self.xfoil_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .ok()
            .expect("Failed to execute Xfoil");

        let mut stdin = child
            .stdin
            .as_mut()
            .expect("Failed to retrieve handle to child stdin");

        for cmd in self.command_sequence.iter() {
            Self::write_to_xfoil(&mut stdin, &cmd)?;
            Self::write_to_xfoil(&mut stdin, "\n")?;
        }

        // If the calculation did not convergence, return ConvergenceError
        let output = child.wait_with_output().unwrap();
        if let Some(_) = String::from_utf8(output.stdout)?
            .as_str()
            .lines()
            .find(|&line| line == " VISCAL:  Convergence failed")
        {
            return Err(error::XfoilError::ConvergenceError);
        }

        if let Some(polar) = &self.polar {
            self.parse_polar(polar)
        } else {
            Ok(HashMap::new())
        }
    }

    fn write_to_xfoil(stdin: &mut ChildStdin, command: &str) -> error::Result<()> {
        Ok(stdin.write_all(command.as_bytes())?)
    }

    fn parse_polar(&self, path: &str) -> error::Result<HashMap<String, Vec<f64>>> {
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
            for (header, value) in table_header.iter().zip(data) {
                result
                    .get_mut::<String>(&header.to_string())
                    .expect("Failed to retrieve result HashMap")
                    .push(value);
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const POLAR_KEYS: [&str; 7] = ["alpha", "CL", "CD", "CDp", "CM", "Top_Xtr", "Bot_Xtr"];

    #[test]
    #[should_panic]
    fn no_foil() {
        let _runner = Config::new("/usr/local/bin/xfoil").get_runner().unwrap();
    }

    #[test]
    fn convergence_error() {
        let result = Config::new("/usr/local/bin/xfoil")
            .naca("2414")
            .reynolds(1)
            .get_runner()
            .unwrap()
            .dispatch();
        assert!(result.is_err(), "Convergence error test did not return Err");
    }

    #[test]
    fn load_airfoil_dat() {
        let results = Config::new("/usr/local/bin/xfoil")
            .airfoil_polar_file("examples/clarky.dat")
            .angle_of_attack(4.0)
            .pacc_random()
            .get_runner()
            .unwrap()
            .dispatch()
            .unwrap();
        let expect_results = [4.000, 0.8965, 0.00000, -0.00118, -0.0942, 0.0000, 0.0000];
        for (&key, &value) in POLAR_KEYS.iter().zip(expect_results.iter()) {
            let val = results.get(&key.to_string()).unwrap();
            assert!((val[0] - value).abs() < 1e-2);
        }
    }

    #[test]
    fn aoa_inertial_success() {
        let results = Config::new("/usr/local/bin/xfoil")
            .naca("2414")
            .angle_of_attack(4.0)
            .pacc_random()
            .get_runner()
            .unwrap()
            .dispatch()
            .unwrap();
        let expect_results = [4.0, 0.7492, 0.0, -0.00131, -0.0633, 0.0, 0.0];
        for (&key, &value) in POLAR_KEYS.iter().zip(expect_results.iter()) {
            let val = results.get(&key.to_string()).unwrap();
            assert!((val[0] - value).abs() < 1e-2);
        }
    }

    #[test]
    fn cl_inertial_success() {
        let results = Config::new("/usr/local/bin/xfoil")
            .naca("2414")
            .lift_coefficient(1.0)
            .pacc_random()
            .get_runner()
            .unwrap()
            .dispatch()
            .unwrap();
        let expect_results = [6.059, 1.0000, 0.00000, -0.00133, -0.0671, 0.0000, 0.0000];
        for (&key, &value) in POLAR_KEYS.iter().zip(expect_results.iter()) {
            let val = results.get(&key.to_string()).unwrap();
            assert!((val[0] - value).abs() < 1e-2);
        }
    }

    #[test]
    fn aoa_viscous_success() {
        let results = Config::new("/usr/local/bin/xfoil")
            .naca("2414")
            .angle_of_attack(4.0)
            .reynolds(100_000)
            .pacc_random()
            .get_runner()
            .unwrap()
            .dispatch()
            .unwrap();
        let expect_results = [4.000, 0.7278, 0.01780, 0.00982, -0.0614, 0.6233, 1.0000];
        for (&key, &value) in POLAR_KEYS.iter().zip(expect_results.iter()) {
            let val = results.get(&key.to_string()).unwrap();
            assert!((val[0] - value).abs() < 1e-2);
        }
    }

    #[test]
    fn cl_viscous_success() {
        let results = Config::new("/usr/local/bin/xfoil")
            .naca("2414")
            .lift_coefficient(1.0)
            .reynolds(100_000)
            .pacc_random()
            .get_runner()
            .unwrap()
            .dispatch()
            .unwrap();
        let expect_results = [7.121, 1.0000, 0.02106, 0.01277, -0.0443, 0.4234, 1.0000];
        for (&key, &value) in POLAR_KEYS.iter().zip(expect_results.iter()) {
            let val = results.get(&key.to_string()).unwrap();
            assert!((val[0] - value).abs() < 1e-2);
        }
    }

    #[test]
    fn create_polar_file() {
        use std::fs::remove_file;
        use std::path::Path;
        let file = "xfoil_create_polar_file_test";
        let _ = Config::new("/usr/local/bin/xfoil")
            .naca("2414")
            .pacc_from_str(file)
            .get_runner()
            .unwrap()
            .dispatch()
            .unwrap();
        assert!(Path::new(file).exists());
        remove_file(Path::new(file)).unwrap();
    }
}
