mod utils;

use clap::{Args as ClapArgs, Parser, Subcommand};
use foxil::result::AnalysisResult;
use foxil::FoxConfig;
use std::path::PathBuf;

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
    xfoil_path: PathBuf,

    /// Path for polar data output directory (used by sweep). If not specified, a temporary file will be used.
    #[arg(
        short = 'p',
        long,
        global = true,
        env = "XFOIL_POLAR_PATH",
        default_value = "./polar.out"
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
    #[arg(long, default_value_t = 20.0)]
    max_aoa: f64,

    /// Angle of attack step for sweep (degrees).
    #[arg(long, default_value_t = 0.1)]
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
    #[arg(long, default_value_t = 20.0, alias = "max-alpha")]
    max_aoa: f64,

    /// Angle of attack step for Cl calculation sweep (degrees).
    #[arg(long, default_value_t = 0.1, alias = "alpha-step")]
    aoa_step: f64,

    /// Output CSV file path for AoA and Cl data.
    #[arg(short = 'o', long, default_value = "cl_data.csv")]
    output_csv: String,
}

fn handle_sweep_command(
    xfoil_path: &PathBuf,
    polar_path: &PathBuf,
    args: &SweepArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Analyzing NACA {} at Re = {} from AoA {:.1}° to {:.1}° (step {:.2}°)...",
        args.naca, args.reynolds, args.min_aoa, args.max_aoa, args.aoa_step
    );

    let analysis_result: AnalysisResult = FoxConfig::new(xfoil_path)
        .aoa_range(args.min_aoa, args.max_aoa, args.aoa_step)
        .polar_accumulation(polar_path)
        .reynolds(args.reynolds as usize)
        .naca(args.naca.as_str())
        .get_runner()
        .expect("Failed to create runner")
        .dispatch()
        .expect("Failed to dispatch")
        .export()
        .into_iter()
        .max_by(|a, b| a.ld_ratio.total_cmp(&b.ld_ratio))
        .expect("No valid analysis result found!");
    utils::display_analysis_summary(args, &analysis_result);
    Ok(())
}

fn handle_get_cl_command(
    xfoil_path: &PathBuf,
    polar_path: &PathBuf,
    args: &GetClArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Calculating Cl for NACA {} at Re = {} from AoA {:.2}° to {:.2}° (step {:.2}°)...",
        args.naca, args.reynolds, args.min_aoa, args.max_aoa, args.aoa_step
    );

    let results = FoxConfig::new(xfoil_path)
        .aoa_range(args.min_aoa, args.max_aoa, args.aoa_step)
        .polar_accumulation(polar_path)
        .reynolds(args.reynolds as usize)
        .naca(args.naca.as_str())
        .get_runner()
        .expect("Failed to create runner")
        .dispatch()
        .expect("Failed to dispatch")
        .export();
    results
        .iter()
        .max_by(|a, b| a.ld_ratio.total_cmp(&b.ld_ratio)); // Note: this max_by result is not used.
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
