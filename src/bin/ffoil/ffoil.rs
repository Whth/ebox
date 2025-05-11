mod utils;

// Still used by Sweep command
use crate::utils::setup_progress_bar;
use clap::{Args as ClapArgs, Parser, Subcommand};
// Added for CSV output in GetCl command
use foxil::Config as XfoilConfig;
use indicatif::ParallelProgressIterator;
use rayon::prelude::*;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use utils::{AnalysisResult, XfoilResult};
// For remove_file and potentially other file ops

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "XFoil CLI for airfoil analysis",
    long_about = "Performs aerodynamic analysis on airfoils using XFoil. Supports sweeping angles of attack or getting Cl for a specific angle."
)]
struct Cli {
    /// Path to the XFoil executable.
    #[arg(
        short = 'x',
        long,
        global = true,
        default_value = "xfoil",
        env = "XFOIL_PATH"
    )]
    xfoil_path: String,

    /// Path for polar data output directory (used by sweep).
    #[arg(
        short = 'p',
        long,
        global = true,
        default_value = "polar.out",
        env = "XFOIL_POLAR_PATH"
    )]
    polar_path: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Calculates aerodynamic coefficients by sweeping through a range of angles of attack
    /// and identifies the angle with the best lift-to-drag ratio.
    Sweep(SweepArgs),
    /// Calculates the lift coefficient (Cl) for a given airfoil over a range of
    /// angles of attack and outputs the results to a CSV file.
    GetCl(GetClArgs),
}

#[derive(Debug, ClapArgs)]
struct SweepArgs {
    /// NACA airfoil designation (e.g., "2412", "0012").
    #[arg(short, long)]
    naca: String,

    /// Reynolds number.
    #[arg(short, long, default_value_t = 1_000_000)]
    reynolds: u32,

    /// Minimum angle of attack for sweep (degrees).
    #[arg(long, default_value_t = -5.0)]
    min_aoa: f64,

    /// Maximum angle of attack for sweep (degrees).
    #[arg(long, default_value_t = 15.0)]
    max_aoa: f64,

    /// Angle of attack step for sweep (degrees).
    #[arg(long, default_value_t = 0.3)]
    aoa_step: f64,
}

#[derive(Debug, ClapArgs)]
struct GetClArgs {
    /// NACA airfoil designation (e.g., "2412", "0012").
    #[arg(short, long)]
    naca: String,

    /// Reynolds number.
    #[arg(short, long, default_value_t = 1_000_000)]
    reynolds: u32,

    /// Minimum angle of attack for Cl calculation sweep (degrees).
    #[arg(long, default_value_t = -5.0, alias = "min-alpha")]
    min_aoa: f64,

    /// Maximum angle of attack for Cl calculation sweep (degrees).
    #[arg(long, default_value_t = 15.0, alias = "max-alpha")]
    max_aoa: f64,

    /// Angle of attack step for Cl calculation sweep (degrees).
    #[arg(long, default_value_t = 0.3, alias = "alpha-step")]
    aoa_step: f64,

    /// Output CSV file path for AoA and Cl data.
    #[arg(short = 'o', long, default_value = "cl_data.csv")]
    output_csv: String,
}

struct RangeSolver {
    aoa_seq: Vec<f64>,
    reynolds: usize,
    polar_dir: PathBuf,
    xfoil_path: String,
    naca: String,
}

impl RangeSolver {
    pub fn new(
        aoa_min: f64,
        aoa_max: f64,
        aoa_step: f64,
        reynolds: usize,
        polar_dir: PathBuf,
        xfoil_path: String,
        naca: String,
    ) -> Self {
        let num_steps = ((aoa_max - aoa_min) / aoa_step).ceil() as usize;
        Self {
            aoa_seq: (0..=num_steps)
                .map(|i| aoa_min + i as f64 * aoa_step)
                .collect(),
            reynolds,
            polar_dir,
            xfoil_path,
            naca,
        }
    }
    pub fn solve(&self) -> Vec<AnalysisResult> {
        fs::create_dir_all(&self.polar_dir).expect("Failed to create polar directory");

        self.aoa_seq
            .par_iter()
            .progress_with(setup_progress_bar(self.aoa_seq.len() as u64, "Solving"))
            .map(|&aoa| {
                XfoilConfig::new(&self.xfoil_path)
                    .polar_accumulation(
                        &self
                            .polar_dir
                            .join(format!("{}_{:.2}.dat", self.naca, aoa))
                            .to_str()
                            .unwrap(),
                    )
                    .naca(&self.naca)
                    .reynolds(self.reynolds)
                    .angle_of_attack(aoa)
                    .get_runner()
                    .expect("Failed to create XFoilRunner")
                    .dispatch()
                    .map(|xfoil_output| {
                        serde_json::from_value::<XfoilResult>(json!(xfoil_output))
                            .expect("Failed to parse Xfoil output")
                    })
                    .expect("Failed to dispatch XFoilRunner")
                    .get_analysis_result(aoa)
            })
            .collect()
    }
}

fn handle_sweep_command(
    xfoil_path: &str,
    polar_path: &PathBuf,
    args: &SweepArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match fs::remove_dir_all(polar_path) {
        Ok(_) => {
            // Directory and its contents removed successfully.
        }
        Err(e) => {
            // If removal failed, but the error is anything other than NotFound,
            // it's a problem we should report.
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(format!(
                    "Failed to delete existing polar directory '{}': {}",
                    polar_path.display(),
                    e
                )
                .into());
            }
            // If the error is NotFound, the directory is already gone, which is acceptable.
        }
    }

    println!(
        "Analyzing NACA {} at Re = {} from AoA {:.1}° to {:.1}° (step {:.2}°)...",
        args.naca, args.reynolds, args.min_aoa, args.max_aoa, args.aoa_step
    );

    let analysis_result = RangeSolver::new(
        args.min_aoa,
        args.max_aoa,
        args.aoa_step,
        args.reynolds as usize,
        polar_path.clone(),
        xfoil_path.to_string(),
        args.naca.clone(),
    )
    .solve()
    .into_iter()
    .filter(|r| r.is_valid())
    .max_by(|a, b| a.ld_ratio.total_cmp(&b.ld_ratio))
    .expect("No valid analysis result found!");
    utils::display_analysis_summary(args, &analysis_result);
    Ok(())
}

fn handle_get_cl_command(
    xfoil_path: &str,
    polar_path: &PathBuf,

    args: &GetClArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Calculating Cl for NACA {} at Re = {} from AoA {:.2}° to {:.2}° (step {:.2}°)...",
        args.naca, args.reynolds, args.min_aoa, args.max_aoa, args.aoa_step
    );

    let results = RangeSolver::new(
        args.min_aoa,
        args.max_aoa,
        args.aoa_step,
        args.reynolds as usize,
        polar_path.clone(),
        xfoil_path.to_string(),
        args.naca.to_string(),
    )
    .solve();
    results
        .iter()
        .max_by(|a, b| a.ld_ratio.total_cmp(&b.ld_ratio));
    println!("Writing results to {}...", args.output_csv);
    let mut wtr = csv::WriterBuilder::new()
        .from_path(&args.output_csv)
        .expect("Error creating CSV file");
    wtr.write_record(&["aoa", "cl", "cd", "ld"])
        .expect("Error writing CSV header");

    for result in results {
        wtr.write_record(&[
            &result.aoa.to_string(),
            &result.cl.to_string(),
            &result.cd.to_string(),
            &result.ld_ratio.to_string(),
        ])?;
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = utils::parse_and_validate_cli()?;

    match &cli.command {
        Commands::Sweep(args) => handle_sweep_command(&cli.xfoil_path, &cli.polar_path, args)?,
        Commands::GetCl(args) => handle_get_cl_command(&cli.xfoil_path, &cli.polar_path, args)?,
    }

    Ok(())
}
