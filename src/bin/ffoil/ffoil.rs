mod utils;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::PathBuf;

use clap::{Args as ClapArgs, Parser, Subcommand};
use foxil::result::AnalysisResult;
use foxil::FoxConfig;
// serde_json is used, IDE will handle import or it's part of common project dependencies.

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
    /// Searches a range of NACA 4-digit airfoils to find the best angle of attack for each,
    /// outputting results to a JSON file.
    SearchNaca(SearchNacaArgs),
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

#[derive(Debug, ClapArgs)]
struct SearchNacaArgs {
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

    /// Output JSON file path for NACA code and best AoA data.
    #[arg(short = 'o', long, default_value = "naca_search_results.json")]
    output_json: String,

    /// Max camber percentages (M) for NACA 4-digit series (e.g., "0,2,4").
    #[arg(long, value_parser = clap::value_parser!(u8), num_args = 1.., value_delimiter = ',', default_value = "0,1,2,3,4,5,6,7,8,9")]
    camber_percent: Vec<u8>,

    /// Position of max camber in tenths of chord (P) for NACA 4-digit series (e.g., "2,4").
    #[arg(long, value_parser = clap::value_parser!(u8), num_args = 1.., value_delimiter = ',', default_value = "1,2,3,4,5,6,7,8,9")]
    camber_pos: Vec<u8>,

    /// Max thickness percentages (XX) for NACA 4-digit series (e.g., "06,12,18").
    #[arg(long, value_parser = clap::value_parser!(u8), num_args = 1.., value_delimiter = ',', default_value = "06,09,12,15,18")]
    thickness_percent: Vec<u8>,
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

fn handle_search_naca_command(
    xfoil_path: &PathBuf,
    polar_path: &PathBuf,
    args: &SearchNacaArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut results_map: HashMap<String, f64> = HashMap::new();

    println!(
        "Starting NACA search: Re={}, AoA range [{:.1}°, {:.1}°], step {:.2}°",
        args.reynolds, args.min_aoa, args.max_aoa, args.aoa_step
    );
    println!(
        "NACA 4-digit Generation Parameters -- Camber (%): {:?}, Position (tenths): {:?}, Thickness (%): {:?}",
        args.camber_percent, args.camber_pos, args.thickness_percent
    );

    let mut naca_codes_to_process: HashSet<String> = HashSet::new();

    for m_val in &args.camber_percent {
        if *m_val == 0 {
            // Symmetric airfoils: NACA 00XX
            for xx_val in &args.thickness_percent {
                if *xx_val == 0 {
                    // Zero thickness airfoil is not practically useful / processable by XFoil
                    continue;
                }
                naca_codes_to_process.insert(format!("00{:02}", xx_val));
            }
        } else {
            // Cambered airfoils: NACA MPXX
            for p_val in &args.camber_pos {
                for xx_val in &args.thickness_percent {
                    if *xx_val == 0 {
                        continue;
                    }
                    naca_codes_to_process.insert(format!("{}{:01}{:02}", m_val, p_val, xx_val));
                }
            }
        }
    }

    let total_nacas = naca_codes_to_process.len();
    println!("Generated {} unique NACA codes to process.", total_nacas);
    let mut count = 0;

    for naca_code_str in naca_codes_to_process {
        count += 1;
        println!(
            "[{}/{}] Processing NACA {}...",
            count, total_nacas, naca_code_str
        );

        let runner_result = FoxConfig::new(xfoil_path)
            .aoa_range(args.min_aoa, args.max_aoa, args.aoa_step)
            .polar_accumulation(polar_path)
            .reynolds(args.reynolds as usize)
            .naca(&naca_code_str)
            .get_runner();

        let xfoil_runner = match runner_result {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "Failed to create XFoil runner for {}: {}. Skipping.",
                    naca_code_str, e
                );
                continue;
            }
        };

        let dispatch_result = xfoil_runner.dispatch();

        let analysis_points = match dispatch_result {
            Ok(res) => res.export(),
            Err(e) => {
                eprintln!(
                    "XFoil dispatch failed for {}: {}. This might be due to non-convergence or invalid airfoil geometry. Skipping.",
                    naca_code_str, e
                );
                continue;
            }
        };

        if analysis_points.is_empty() {
            println!(
                "No valid analysis points for NACA {}. This could be due to all points failing to converge. Skipping.",
                naca_code_str
            );
            continue;
        }

        let best_result_opt = analysis_points
            .into_iter()
            .filter(|ap| ap.ld_ratio.is_finite() && ap.ld_ratio > 0.0) // Consider only valid, positive L/D ratios
            .max_by(|a, b| a.ld_ratio.total_cmp(&b.ld_ratio));

        if let Some(best_result) = best_result_opt {
            println!(
                "NACA {}: Best L/D {:.2} at AoA {:.2}° (Cl={:.3}, Cd={:.4})",
                naca_code_str,
                best_result.ld_ratio,
                best_result.aoa,
                best_result.cl,
                best_result.cd
            );
            results_map.insert(naca_code_str.clone(), best_result.aoa);
        } else {
            println!(
                "Could not find a best L/D (with L/D > 0) for NACA {}. Skipping.",
                naca_code_str
            );
        }
    }

    println!("Writing search results to {}...", args.output_json);
    let file = File::create(&args.output_json)?;
    serde_json::to_writer_pretty(file, &results_map)?;

    println!(
        "NACA search completed. Results saved to {}.",
        args.output_json
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Sweep(args) => handle_sweep_command(&cli.xfoil_path, &cli.polar_path, args)?,
        Commands::GetCl(args) => handle_get_cl_command(&cli.xfoil_path, &cli.polar_path, args)?,
        Commands::SearchNaca(args) => {
            handle_search_naca_command(&cli.xfoil_path, &cli.polar_path, args)?
        }
    }

    Ok(())
}
