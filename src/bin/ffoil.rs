use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rs_xfoil::Config as XfoilConfig;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "XFoil CLI for airfoil analysis",
    long_about = "Calculates aerodynamic coefficients for a given NACA airfoil by sweeping through a range of angles of attack and identifies the angle with the best lift-to-drag ratio."
)]
struct Args {
    /// Path to the XFoil executable.
    #[arg(short = 'x', long, default_value = "xfoil", env = "XFOIL_PATH")]
    xfoil_path: String,

    /// Path for polar data output file.
    #[arg(short = 'p', long, default_value = "polar.out", env = "XFOIL_POLAR_PATH")]
    polar_path: String,

    /// Skip deletion of existing polar file
    #[arg(short = 'd', long, default_value_t = false)]
    no_delete: bool,

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
    #[arg(long, default_value_t = 0.5)]
    aoa_step: f64,
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

fn parse_and_validate_args() -> Result<Args, Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.aoa_step <= 1e-6 {
        eprintln!(
            "Error: Angle of attack step must be positive and greater than a small threshold."
        );
        return Err("Invalid AoA step value.".into());
    }
    if args.max_aoa < args.min_aoa {
        eprintln!("Error: Maximum AoA must be greater than or equal to Minimum AoA.");
        return Err("Maximum AoA constraint violated.".into());
    }

    if !Path::new(&args.xfoil_path).exists() {
        eprintln!(
            "Error: XFoil executable not found at '{}'.",
            args.xfoil_path
        );
        eprintln!("Please specify the correct path using --xfoil-path, the XFOIL_PATH environment variable, or ensure it's in the default location.");
        return Err(format!("XFoil executable not found: {}", args.xfoil_path).into());
    }
    Ok(args)
}

fn setup_progress_bar(num_steps: u64) -> ProgressBar {
    let pb = ProgressBar::new(num_steps);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) AoA: {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar()) // Fallback if template is invalid
            .progress_chars("##-"),
    );
    pb
}

fn process_xfoil_output(
    xfoil_output: &HashMap<String, Vec<f64>>,
    current_aoa: f64,
    analysis_stats: &mut AnalysisResult,
) {
    // Attempt to get the last CL and CD values from the vectors.
    // If polar accumulation appends (which XFoil's PACC does),
    // the last entry corresponds to the most recent calculation (current_aoa).
    let cl_opt = xfoil_output.get("CL").and_then(|v| v.last()).copied();
    let cd_opt = xfoil_output.get("CD").and_then(|v| v.last()).copied();

    // If both CL and CD values are present for the last point,
    // it implies the calculation for current_aoa converged and was recorded.
    if let (Some(cl), Some(cd)) = (cl_opt, cd_opt) {
        // The polar file typically only contains converged points.
        // Thus, presence of data for CL and CD implies convergence.
        if cd.abs() > 1e-9 {
            // Use abs() for cd to handle potential negative (though unusual) drag. More robustly, check cd > 0.
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
    // If CL or CD is not found (e.g., xfoil_output is empty because no polar was written,
    // or the specific keys "CL", "CD" are missing, or their vectors are empty),
    // this specific AoA point is not processed further. This implicitly handles non-convergence
    // if non-converged points are not written to the polar file by XFoil.
}

fn perform_xfoil_sweep(
    args: &Args,
    pb: &ProgressBar,
) -> Result<AnalysisResult, Box<dyn std::error::Error>> {
    let num_steps = ((args.max_aoa - args.min_aoa) / args.aoa_step).floor() as usize + 1;
    let aoa_list: Vec<f64> = (0..num_steps)
        .map(|i| args.min_aoa + i as f64 * args.aoa_step)
        .collect();

    pb.set_length(num_steps as u64);

    let results: Vec<AnalysisResult> = aoa_list
        .par_iter()
        .map(|&current_aoa| {
            pb.set_message(format!("{:.2}°", current_aoa));

            let config = XfoilConfig::new(&args.xfoil_path)
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

            let mut result = AnalysisResult::default();
            match runner.dispatch() {
                Ok(xfoil_output) => {
                    process_xfoil_output(&xfoil_output, current_aoa, &mut result);
                }
                Err(e) => {
                    pb.println(format!(
                        "Warning: XFoil execution failed for AoA = {:.2}°: {}",
                        current_aoa, e
                    ));
                }
            }

            pb.inc(1);
            result
        })
        .collect();

    // Merge results from parallel computations
    let mut final_result = AnalysisResult::default();
    for result in results {
        if result.found_valid_result && result.max_cl_cd_ratio > final_result.max_cl_cd_ratio {
            final_result = result;
        }
    }

    Ok(final_result)
}

fn display_analysis_summary(args: &Args, result: &AnalysisResult) {
    if result.found_valid_result && result.best_aoa.is_some() {
        println!("\n--- Optimal Aerodynamic Performance ---");
        println!("Airfoil: NACA {}", args.naca);
        println!("Reynolds Number: {}", args.reynolds);
        println!(
            "Best Angle of Attack (for max Cl/Cd): {:.2}°",
            result.best_aoa.unwrap() // Safe due to check
        );
        println!("Lift Coefficient (Cl) at best AoA: {:.4}", result.best_cl);
        println!("Drag Coefficient (Cd) at best AoA: {:.4}", result.best_cd);
        println!("Maximum Cl/Cd Ratio: {:.4}", result.max_cl_cd_ratio);
    } else {
        println!(
            "\nNo suitable aerodynamic performance data found within the specified AoA range."
        );
        println!("This could be due to non-convergence across all angles or characteristics of the airfoil at this Reynolds number.");
        println!("Consider adjusting AoA range/step, Reynolds number, or checking XFoil's behavior manually for this case.");
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_and_validate_args()?;

    if !args.no_delete {
        if let Err(e) = std::fs::remove_file(&args.polar_path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                panic!("Failed to delete existing polar file: {}", e);
            }
        }
    }

    println!(
        "Analyzing NACA {} at Re = {} from AoA {:.1}° to {:.1}° (step {:.2}°)...",
        args.naca, args.reynolds, args.min_aoa, args.max_aoa, args.aoa_step
    );

    let num_steps = ((args.max_aoa - args.min_aoa) / args.aoa_step).floor() as u64 + 1;
    let pb = setup_progress_bar(num_steps);

    let analysis_result = perform_xfoil_sweep(&args, &pb)?;

    pb.finish_with_message("Analysis complete");

    display_analysis_summary(&args, &analysis_result);

    Ok(())
}
