mod utils;

use std::collections::HashSet;
use std::path::PathBuf;

use crate::utils::setup_progress_bar;
use clap::{Args as ClapArgs, Parser, Subcommand};
use foxil::result::AnalysisResult;
use foxil::FoxConfig;
use indicatif::ParallelProgressIterator;
use rayon::prelude::*;
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

    Load(LoadArgs),
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

    /// Output CSV file path.
    #[arg(short, long, required = false)]
    output_csv: Option<PathBuf>,
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
    #[arg(default_value = "search_output")]
    bulk_dir: PathBuf,

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
    #[arg(short = 'o', long, default_value = "naca_search_results.csv")]
    output_csv: String,

    /// Max camber percentages (M) for NACA 4-digit series (e.g., "0,2,4").
    #[arg(long, value_parser = clap::value_parser!(u8), num_args = 1.., value_delimiter = ',', default_value = "0,1,2,3,4,5,6,7,8,9"
    )]
    camber_percent: Vec<u8>,

    /// Position of max camber in tenths of chord (P) for NACA 4-digit series (e.g., "2,4").
    #[arg(long, value_parser = clap::value_parser!(u8), num_args = 1.., value_delimiter = ',', default_value = "1,2,3,4,5,6,7,8,9"
    )]
    camber_pos: Vec<u8>,

    /// Max thickness percentages (XX) for NACA 4-digit series (e.g., "06,12,18").
    #[arg(long, value_parser = clap::value_parser!(u8), num_args = 1.., value_delimiter = ',', default_value = "01,02,03,04,05,06,07,08,09,\
        10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,\
        56,57,58,59,60,61,62,63,64,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,91,92,93,94,95,96,97,98,99"
    )]
    thickness_percent: Vec<u8>,
}

#[derive(Debug, ClapArgs)]
struct LoadArgs {
    input: PathBuf,
    #[arg(short = 'o', long, default_value = "foil_data.csv")]
    output: PathBuf,
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
        .dispatch()?
        .get_output()
        .map(|out| {
            if let Some(path) = &args.output_csv {
                println!("Writing results to {}", path.display());
                out.to_csv(path).expect("Failed to write CSV")
            } else {
                out
            }
        })?
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
        .dispatch()?
        .get_output()?
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

fn generate_naca_codes(args: &SearchNacaArgs) -> HashSet<String> {
    let symmetric_codes = args
        .camber_percent
        .iter()
        .filter(|&&m_val| m_val == 0) // Only take m=0 for symmetric
        .flat_map(|_m_val_is_zero| {
            args.thickness_percent
                .iter()
                .filter(|&&xx_val| xx_val != 0) // Thickness must be non-zero
                .map(|&xx_val| format!("00{:02}", xx_val))
        });

    let cambered_codes = args
        .camber_percent
        .iter()
        .filter(|&&m_val| m_val != 0) // Only take m > 0 for cambered
        .flat_map(|&m_val| {
            args.camber_pos.iter().flat_map(move |&p_val| {
                args.thickness_percent
                    .iter()
                    .filter(|&&xx_val| xx_val != 0) // Thickness must be non-zero
                    .map(move |&xx_val| format!("{}{:01}{:02}", m_val, p_val, xx_val))
            })
        });

    symmetric_codes.chain(cambered_codes).collect()
}

// Helper struct for storing the best aerodynamic performance data for a NACA airfoil
#[derive(Debug, Clone)]
struct NacaBestAerodynamicPerformance {
    naca_code: String,
    aoa: f64,
    cl: f64,
    cd: f64,
    ld_ratio: f64,
}

fn analyze_single_naca(
    naca_code_str: &str,
    xfoil_path: &PathBuf,
    search_args: &SearchNacaArgs,
) -> Option<NacaBestAerodynamicPerformance> {
    let runner_result = FoxConfig::new(xfoil_path)
        .aoa_range(
            search_args.min_aoa,
            search_args.max_aoa,
            search_args.aoa_step,
        )
        .polar_accumulation(search_args.bulk_dir.join(format!("{}", naca_code_str))) // Unique polar file for this NACA
        .reynolds(search_args.reynolds as usize)
        .naca(naca_code_str)
        .get_runner();

    let xfoil_runner = match runner_result {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "Failed to create XFoil runner for NACA {}: {}. Skipping.",
                naca_code_str, e
            );
            return None;
        }
    };

    let dispatch_result = xfoil_runner.dispatch();

    let analysis_points = match dispatch_result {
        Ok(dispatched_runner) => match dispatched_runner.get_output() {
            Ok(output) => output.export(),
            Err(e) => {
                eprintln!(
                    "Failed to get output for NACA {}: {}. This might be due to non-convergence or invalid airfoil geometry. Skipping.",
                    naca_code_str, e
                );
                return None;
            }
        },
        Err(e) => {
            eprintln!(
                "XFoil dispatch command failed for NACA {}: {}. Skipping.",
                naca_code_str, e
            );
            return None;
        }
    };

    if analysis_points.is_empty() {
        println!(
            "No valid analysis points for NACA {}. This could be due to all points failing to converge. Skipping.",
            naca_code_str
        );
        return None;
    }

    analysis_points
        .into_iter()
        .filter(|ap| ap.ld_ratio.is_finite() && ap.ld_ratio > 0.0) // Ensure L/D is valid and positive
        .max_by(|a, b| a.ld_ratio.total_cmp(&b.ld_ratio))
        .map(|best_result| {
            println!(
                "NACA {}: Best L/D {:.2} at AoA {:.2}° (Cl={:.3}, Cd={:.4})",
                naca_code_str,
                best_result.ld_ratio,
                best_result.aoa,
                best_result.cl,
                best_result.cd
            );
            NacaBestAerodynamicPerformance {
                naca_code: naca_code_str.to_string(),
                aoa: best_result.aoa,
                cl: best_result.cl,
                cd: best_result.cd,
                ld_ratio: best_result.ld_ratio,
            }
        })
        .or_else(|| {
            println!(
                "Could not find a best L/D (with L/D > 0 and finite) for NACA {}. Skipping.",
                naca_code_str
            );
            None
        })
}

fn write_naca_search_results_to_csv(
    results: &[NacaBestAerodynamicPerformance],
    output_csv_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Writing search results to {}...", output_csv_path);
    let mut wtr = csv::WriterBuilder::new().from_path(output_csv_path)?;

    wtr.write_record(&[
        "naca_code",
        "best_aoa",
        "cl_at_best_aoa",
        "cd_at_best_aoa",
        "max_ld_ratio",
    ])?;

    for record in results {
        wtr.write_record(&[
            &record.naca_code,
            &record.aoa.to_string(),
            &record.cl.to_string(),
            &record.cd.to_string(),
            &format!("{:.5}", record.ld_ratio),
        ])?;
    }
    wtr.flush()?;

    println!(
        "NACA search completed. Results saved to {}.",
        output_csv_path
    );
    Ok(())
}

fn handle_search_naca_command(
    xfoil_path: &PathBuf,
    args: &SearchNacaArgs, // args.output_json will be used as the CSV file path
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Starting NACA search: Re={}, AoA range [{:.1}°, {:.1}°], step {:.2}°",
        args.reynolds, args.min_aoa, args.max_aoa, args.aoa_step
    );
    println!(
        "NACA 4-digit Generation Parameters -- Camber (%): {:?}, Position (tenths): {:?}, Thickness (%): {:?}",
        args.camber_percent, args.camber_pos, args.thickness_percent
    );

    let naca_codes_to_process = generate_naca_codes(args);
    let total_nacas = naca_codes_to_process.len();

    if total_nacas == 0 {
        println!("No NACA codes generated based on the input parameters. Nothing to process.");
        return Ok(());
    }
    println!("Generated {} unique NACA codes to process.", total_nacas);

    let best_performances: Vec<NacaBestAerodynamicPerformance> = naca_codes_to_process
        .par_iter()
        .progress_with(setup_progress_bar(total_nacas as u64, "Searching best AoA"))
        .filter_map(|naca_code| analyze_single_naca(naca_code, xfoil_path, args))
        .collect();

    // Use the output_json field from SearchNacaArgs as the path for the CSV file.
    // The name of the field in SearchNacaArgs is assumed to remain `output_json` for now,
    // but its help text and purpose have effectively changed.
    write_naca_search_results_to_csv(&best_performances, &args.output_csv)?;

    Ok(())
}

fn handle_load(xfoil_path: &PathBuf, args: &LoadArgs) -> Result<(), Box<dyn std::error::Error>> {
    FoxConfig::new(xfoil_path)
        .polar_accumulation(&args.input)
        .get_runner()?
        .get_output()?
        .to_csv(&args.output)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Sweep(args) => handle_sweep_command(&cli.xfoil_path, &cli.polar_path, args)?,
        Commands::GetCl(args) => handle_get_cl_command(&cli.xfoil_path, &cli.polar_path, args)?,
        Commands::SearchNaca(args) => handle_search_naca_command(&cli.xfoil_path, args)?,
        Commands::Load(args) => handle_load(&cli.xfoil_path, args)?,
    }

    Ok(())
}
