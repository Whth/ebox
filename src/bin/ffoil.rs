use clap::{Args as ClapArgs, Parser, Subcommand};
use csv::Writer;
// Added for CSV output in GetCl command
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rs_xfoil::Config as XfoilConfig;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
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
    /// Path for polar data output file (used by sweep).
    #[arg(
        short = 'p',
        long,
        default_value = "polar.out",
        env = "XFOIL_POLAR_PATH"
    )]
    polar_path: String,

    /// Skip deletion of existing polar file.
    #[arg(short = 'd', long, default_value_t = false)]
    no_delete: bool,

    /// NACA airfoil designation (e.g., "2412", "0012").
    #[arg(short, long)]
    naca: String,

    /// Reynolds number.
    #[arg(short, long, default_value_t = 1_000_000)]
    reynolds: u32,

    /// Minimum angle of attack for sweep (degrees).
    #[arg(long, default_value_t = 0.0)]
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
    /// Path for polar data output file (used by XFoil).
    #[arg(
        short = 'p',
        long,
        default_value = "polar.out", // Default can be the same or different
        env = "XFOIL_POLAR_PATH"
    )]
    polar_path: String,

    /// NACA airfoil designation (e.g., "2412", "0012").
    #[arg(short, long)]
    naca: String,

    /// Reynolds number.
    #[arg(short, long, default_value_t = 1_000_000)]
    reynolds: u32,

    /// Minimum angle of attack for Cl calculation sweep (degrees).
    #[arg(long, default_value_t = 0.0, alias = "min-alpha")]
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

struct AnalysisResult {
    best_aoa: Option<f64>,
    best_cl: f64,
    best_cd: f64,
    max_cl_cd_ratio: f64,
    found_valid_result: bool,
}

impl Default for AnalysisResult {
    fn default() -> Self {
        Self {
            best_aoa: None,
            best_cl: 0.0,
            best_cd: 0.0,
            max_cl_cd_ratio: f64::NEG_INFINITY,
            found_valid_result: false,
        }
    }
}

fn validate_xfoil_path(xfoil_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new(xfoil_path).exists() {
        eprintln!("Error: XFoil executable not found at '{}'.", xfoil_path);
        eprintln!("Please specify the correct path using --xfoil-path, the XFOIL_PATH environment variable, or ensure it's in the default location.");
        return Err(format!("XFoil executable not found: {}", xfoil_path).into());
    }
    Ok(())
}

fn parse_and_validate_cli() -> Result<Cli, Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    validate_xfoil_path(&cli.xfoil_path)?;

    match &cli.command {
        Commands::Sweep(args) => {
            if args.aoa_step <= 1e-6 {
                // Use a small epsilon for float comparison
                eprintln!("Error: Angle of attack step must be positive and greater than a small threshold.");
                return Err("Invalid AoA step value.".into());
            }
            if args.max_aoa < args.min_aoa {
                eprintln!("Error: Maximum AoA must be greater than or equal to Minimum AoA.");
                return Err("Maximum AoA constraint violated.".into());
            }
        }
        Commands::GetCl(args) => {
            if args.aoa_step <= 1e-6 {
                // Use a small epsilon for float comparison
                eprintln!("Error: Angle of attack step for GetCl must be positive and greater than a small threshold.");
                return Err("Invalid AoA step value for GetCl.".into());
            }
            if args.max_aoa < args.min_aoa {
                eprintln!(
                    "Error: Maximum AoA for GetCl must be greater than or equal to Minimum AoA."
                );
                return Err("Maximum AoA constraint violated for GetCl.".into());
            }
        }
    }
    Ok(cli)
}

fn setup_progress_bar(num_steps: u64, description: &str) -> ProgressBar {
    let pb = ProgressBar::new(num_steps);
    let template = format!("{{spinner:.green}} [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {{pos}}/{{len}} ({{eta}}) {}: {{msg}}", description);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(&template)
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("##-"),
    );
    pb
}

fn process_xfoil_output(
    xfoil_output: &HashMap<String, Vec<f64>>,
    current_aoa: f64,
    analysis_stats: &mut AnalysisResult,
) {
    let cl_opt = xfoil_output.get("CL").and_then(|v| v.last()).copied();
    let cd_opt = xfoil_output.get("CD").and_then(|v| v.last()).copied();

    if let (Some(cl), Some(cd)) = (cl_opt, cd_opt) {
        if cd.abs() > 1e-9 {
            // Check cd is not effectively zero to avoid division errors
            // More robustly, one might check `cd > 0` if negative drag is impossible/undesired.
            let cl_cd_ratio = cl / cd;
            if cl_cd_ratio > analysis_stats.max_cl_cd_ratio {
                analysis_stats.max_cl_cd_ratio = cl_cd_ratio;
                analysis_stats.best_aoa = Some(current_aoa);
                analysis_stats.best_cl = cl;
                analysis_stats.best_cd = cd;
                analysis_stats.found_valid_result = true;
            }
        }
    }
}

fn perform_xfoil_sweep(
    xfoil_path: &str,
    args: &SweepArgs,
    pb: &ProgressBar,
) -> Result<AnalysisResult, Box<dyn std::error::Error>> {
    let num_steps = ((args.max_aoa - args.min_aoa) / args.aoa_step).floor() as usize + 1;
    let aoa_list: Vec<f64> = (0..num_steps)
        .map(|i| args.min_aoa + i as f64 * args.aoa_step)
        .collect();

    let results: Vec<AnalysisResult> = aoa_list
        .par_iter()
        .map(|&current_aoa| {
            pb.set_message(format!("{:.2}°", current_aoa));

            let config = XfoilConfig::new(xfoil_path)
                .naca(&args.naca)
                .reynolds(args.reynolds as usize)
                .angle_of_attack(current_aoa)
                .polar_accumulation(&args.polar_path);

            let runner = match config.get_runner() {
                Ok(r) => r,
                Err(e) => {
                    pb.println(format!(
                        "Warning: Failed to initialize XFoil runner for AoA = {:.2}°: {}",
                        current_aoa, e
                    ));
                    return AnalysisResult::default();
                }
            };

            let mut result_for_aoa = AnalysisResult::default();
            match runner.dispatch() {
                Ok(xfoil_output) => {
                    process_xfoil_output(&xfoil_output, current_aoa, &mut result_for_aoa);
                }
                Err(e) => {
                    pb.println(format!(
                        "Warning: XFoil execution failed for AoA = {:.2}°: {}",
                        current_aoa, e
                    ));
                }
            }
            pb.inc(1);
            result_for_aoa
        })
        .collect();

    let mut final_result = AnalysisResult::default();
    for result_for_aoa in results {
        if result_for_aoa.found_valid_result
            && result_for_aoa.max_cl_cd_ratio > final_result.max_cl_cd_ratio
        {
            final_result = result_for_aoa;
        } else if result_for_aoa.found_valid_result && !final_result.found_valid_result {
            final_result = result_for_aoa;
        }
    }
    if final_result.best_aoa.is_some() {
        final_result.found_valid_result = true;
    }

    Ok(final_result)
}

fn display_analysis_summary(args: &SweepArgs, result: &AnalysisResult) {
    if result.found_valid_result && result.best_aoa.is_some() {
        println!("\n--- Optimal Aerodynamic Performance (Sweep) ---");
        println!("Airfoil: NACA {}", args.naca);
        println!("Reynolds Number: {}", args.reynolds);
        println!(
            "Best Angle of Attack (for max Cl/Cd): {:.2}°",
            result.best_aoa.unwrap()
        );
        println!("Lift Coefficient (Cl) at best AoA: {:.4}", result.best_cl);
        println!("Drag Coefficient (Cd) at best AoA: {:.4}", result.best_cd);
        println!("Maximum Cl/Cd Ratio: {:.4}", result.max_cl_cd_ratio);
    } else {
        println!(
            "\nNo suitable aerodynamic performance data found within the specified AoA range for sweep."
        );
        println!("This could be due to non-convergence across all angles or characteristics of the airfoil at this Reynolds number.");
        println!("Consider adjusting AoA range/step, Reynolds number, or checking XFoil's behavior manually for this case.");
    }
}

fn run_xfoil_single_aoa(
    xfoil_path: &str,
    naca: &str,
    reynolds: u32,
    aoa: f64,
    polar_path: &str, // Added polar_path argument
) -> Result<Option<f64>, Box<dyn std::error::Error>> {
    let config = XfoilConfig::new(xfoil_path)
        .naca(naca)
        .reynolds(reynolds as usize)
        .angle_of_attack(aoa)
        .polar_accumulation(polar_path); // Use polar_path for XFoil config

    let runner = config.get_runner()?;
    let xfoil_output = runner.dispatch()?;

    let cl_value = xfoil_output.get("CL").and_then(|v| v.first()).copied();
    Ok(cl_value)
}

fn write_cl_data_to_csv(
    output_path: &str,
    naca: &str,
    reynolds: u32,
    data: &[(f64, Option<f64>)], // List of (AoA, Option<Cl>)
) -> Result<(), Box<dyn std::error::Error>> {
    let mut wtr = Writer::from_path(output_path)?;
    wtr.write_record(&["NACA", "Reynolds", "AoA_deg", "Cl"])?;
    for (aoa, cl_opt) in data {
        let cl_str = match cl_opt {
            Some(cl_val) => format!("{:.4}", cl_val),
            None => "N/A".to_string(), // Represent non-converged or error cases
        };
        wtr.write_record(&[
            naca.to_string(),
            reynolds.to_string(),
            format!("{:.2}", aoa),
            cl_str,
        ])?;
    }
    wtr.flush()?;
    println!("Cl data written to {}", output_path);
    Ok(())
}

fn handle_sweep_command(
    xfoil_path: &str,
    args: &SweepArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if !args.no_delete {
        if Path::new(&args.polar_path).exists() {
            if let Err(e) = fs::remove_file(&args.polar_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(format!(
                        "Failed to delete existing polar file '{}': {}",
                        args.polar_path, e
                    )
                    .into());
                }
            }
        }
    }

    println!(
        "Analyzing NACA {} at Re = {} from AoA {:.1}° to {:.1}° (step {:.2}°)...",
        args.naca, args.reynolds, args.min_aoa, args.max_aoa, args.aoa_step
    );

    let num_steps = ((args.max_aoa - args.min_aoa) / args.aoa_step).floor() as u64 + 1;
    let pb = setup_progress_bar(num_steps, "Sweeping AoA");
    pb.set_length(num_steps);

    let analysis_result = perform_xfoil_sweep(xfoil_path, args, &pb)?;

    pb.finish_with_message("Sweep analysis complete");
    display_analysis_summary(args, &analysis_result);
    Ok(())
}

fn handle_get_cl_command(
    xfoil_path: &str,
    args: &GetClArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    // For GetCl, we might not want to delete the polar file by default,
    // or perhaps make it configurable if it's used for temporary storage.
    // If polar_path is critical for GetCl's XFoil runs, ensure it's handled.
    // For now, assuming polar_path is primarily for output in Sweep,
    // but for GetCl, XFoil still needs to write a temporary polar,
    // so ensure that path is valid and doesn't conflict if used concurrently.
    // If rs_xfoil handles temporary polar files internally when polar_accumulation is not set,
    // then GetClArgs.polar_path might be redundant unless we *want* to save a polar for GetCl too.
    // The prompt implies GetCl *needs* it for xfoil to work, so we will use it.
    // We should also consider if we need a `no_delete` for GetCl's polar.
    // For simplicity and following the prompt that "GetCl needs it", we'll assume it's needed.
    // We won't delete it here, assuming rs_xfoil manages or overwrites it.
    // If rs_xfoil *requires* `polar_accumulation` to be called, then we must pass it.

    println!(
        "Calculating Cl for NACA {} at Re = {} from AoA {:.2}° to {:.2}° (step {:.2}°)...",
        args.naca, args.reynolds, args.min_aoa, args.max_aoa, args.aoa_step
    );

    let num_steps = if args.max_aoa >= args.min_aoa && args.aoa_step > 1e-9 {
        ((args.max_aoa - args.min_aoa) / args.aoa_step).floor() as usize + 1
    } else if (args.max_aoa - args.min_aoa).abs() < 1e-9 && args.aoa_step > 1e-9 {
        // Single point
        1
    } else {
        0 // Will be caught by validation or lead to empty aoa_list
    };

    if num_steps == 0 {
        eprintln!("No AoA steps to process based on the provided range and step. Ensure min_aoa <= max_aoa and aoa_step is positive.");
        return Err("No AoA steps to process.".into());
    }

    let aoa_list: Vec<f64> = (0..num_steps)
        .map(|i| args.min_aoa + i as f64 * args.aoa_step)
        .collect();

    if aoa_list.is_empty() {
        eprintln!("Generated AoA list is empty. Check input parameters.");
        return Err("No AoAs to process after list generation.".into());
    }

    let pb = setup_progress_bar(aoa_list.len() as u64, "Calculating Cl");
    pb.set_length(aoa_list.len() as u64);

    let mut cl_results_with_aoa: Vec<(f64, Option<f64>)> = aoa_list
        .par_iter()
        .map(|&current_aoa| {
            pb.set_message(format!("AoA: {:.2}°", current_aoa));
            // Pass polar_path to run_xfoil_single_aoa
            let cl_value_opt = match run_xfoil_single_aoa(xfoil_path, &args.naca, args.reynolds, current_aoa, &args.polar_path) {
                Ok(cl_opt) => {
                    if cl_opt.is_none() {
                         pb.println(format!(
                            "Warning: No Cl value obtained for AoA = {:.2}° (likely non-convergence).",
                            current_aoa
                        ));
                    }
                    cl_opt
                }
                Err(e) => {
                    pb.println(format!(
                        "Warning: XFoil execution failed for AoA = {:.2}°: {}",
                        current_aoa, e
                    ));
                    None
                }
            };
            pb.inc(1);
            (current_aoa, cl_value_opt)
        })
        .collect();

    pb.finish_with_message("Cl calculation complete.");

    cl_results_with_aoa.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let has_any_cl_value = cl_results_with_aoa
        .iter()
        .any(|(_, cl_opt)| cl_opt.is_some());
    if !has_any_cl_value && !cl_results_with_aoa.is_empty() {
        eprintln!("Warning: No Cl values were successfully calculated for any AoA in the specified range. CSV will contain N/A for Cl values.");
    } else if cl_results_with_aoa.is_empty() {
        eprintln!("Error: No data was generated. This indicates an issue with AoA list generation or processing logic.");
        return Err("No Cl data generated due to empty processing list.".into());
    }

    write_cl_data_to_csv(
        &args.output_csv,
        &args.naca,
        args.reynolds,
        &cl_results_with_aoa,
    )?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_and_validate_cli()?;

    match &cli.command {
        Commands::Sweep(args) => handle_sweep_command(&cli.xfoil_path, args)?,
        Commands::GetCl(args) => handle_get_cl_command(&cli.xfoil_path, args)?,
    }

    Ok(())
}
