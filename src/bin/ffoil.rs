use clap::{Args as ClapArgs, Parser, Subcommand};
use csv::Writer;
// Added for CSV output in GetCl command
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
// Still used by Sweep command
use rs_xfoil::Config as XfoilConfig;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::Path;
// For remove_file and potentially other file ops

#[derive(Deserialize)]
struct XfoilResult {
    alpha: Vec<f64>,

    #[serde(rename = "CL")]
    cl: Vec<f64>,
    #[serde(rename = "CD")]
    cd: Vec<f64>,
    #[serde(rename = "CDp")]
    cd_p: Vec<f64>,
    #[serde(rename = "CM")]
    cm: Vec<f64>,
    #[serde(rename = "Top_Xtr")]
    top_xtr: Vec<f64>,
    #[serde(rename = "Bot_Xtr")]
    bot_xtr: Vec<f64>,
}

impl XfoilResult {
    fn get_analysis_result(&self, aoa: f64) -> AnalysisResult {
        if let Some(idx) = self.alpha.iter().position(|&x| x == aoa) {
            let cl = self.cl.get(idx).copied().expect("cl not found!");
            let cd = self.cd.get(idx).copied().expect("cd not found!");
            AnalysisResult::valid_result(aoa, cl, cd)
        } else {
            AnalysisResult::default()
        }
    }
}

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
    polar_path: String,

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
    /// Skip deletion of existing polar directory.
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

#[derive(Default)]
struct AnalysisResult {
    aoa: f64,
    cl: f64,
    cd: f64,
    ld_ratio: f64,
    valid: bool,
}

impl AnalysisResult {
    fn valid_result(aoa: f64, cl: f64, cd: f64) -> Self {
        let ld_ratio = if cd.abs() < 1e-9 { 0.0 } else { cl / cd }; // Avoid division by zero
        AnalysisResult {
            aoa,
            cl,
            cd,
            ld_ratio,
            valid: true,
        }
    }

    fn is_valid(&self) -> bool {
        self.valid
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

fn perform_xfoil_sweep(
    xfoil_path: &str,
    args: &SweepArgs,
    pb: &ProgressBar,
) -> Result<AnalysisResult, Box<dyn std::error::Error>> {
    let num_steps = (((args.max_aoa - args.min_aoa) / args.aoa_step).floor() as usize + 1).max(1); // Ensure at least one step

    let results: Vec<AnalysisResult> = (0..num_steps)
        .map(|i| args.min_aoa + i as f64 * args.aoa_step)
        .collect::<Vec<f64>>()
        .par_iter()
        .map(|&current_aoa| {
            pb.set_message(format!("{:.2}°", current_aoa));

            let config = XfoilConfig::new(xfoil_path)
                .naca(&args.naca)
                .reynolds(args.reynolds as usize)
                .angle_of_attack(current_aoa)
                .polar_accumulation(format!("{}/{:.2}", args.polar_path, current_aoa).as_str());

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

            match runner.dispatch().map(|xfoil_output| {
                serde_json::from_value::<XfoilResult>(json!(xfoil_output))
                    .expect("Failed to parse Xfoil output")
            }) {
                Ok(xfoil_result) => {
                    pb.inc(1);
                    xfoil_result.get_analysis_result(current_aoa)
                }
                Err(e) => {
                    pb.println(format!(
                        "Warning: XFoil execution failed for AoA = {:.2}°: {}",
                        current_aoa, e
                    ));
                    AnalysisResult::default()
                }
            }
        })
        .collect();

    results
        .into_iter()
        .filter(|r| r.is_valid())
        .max_by(|a, b| a.ld_ratio.total_cmp(&b.ld_ratio))
        .ok_or_else(|| "No valid results found to determine optimal performance.".into())
}

fn display_analysis_summary(args: &SweepArgs, result: &AnalysisResult) {
    if result.is_valid() {
        println!("\n--- Optimal Aerodynamic Performance (Sweep) ---");
        println!("Airfoil: NACA {}", args.naca);
        println!("Reynolds Number: {}", args.reynolds);
        println!("Best Angle of Attack (for max Cl/Cd): {:.2}°", result.aoa);
        println!("Lift Coefficient (Cl) at best AoA: {:.4}", result.cl);
        println!("Drag Coefficient (Cd) at best AoA: {:.4}", result.cd);
        println!("Maximum Cl/Cd Ratio: {:.4}", result.ld_ratio);
    } else {
        println!(
            "\nNo suitable aerodynamic performance data found within the specified AoA range for sweep."
        );
        println!("This could be due to non-convergence across all angles or characteristics of the airfoil at this Reynolds number.");
        println!("Consider adjusting AoA range/step, Reynolds number, or checking XFoil's behavior manually for this case.");
    }
}

fn write_cl_data_to_csv(
    output_path: &str,
    naca: &str,
    reynolds: u32,
    data: &[(f64, Option<f64>)],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut wtr = Writer::from_path(output_path)?;
    wtr.write_record(&["NACA", "Reynolds", "AoA_deg", "Cl"])?;
    if data.is_empty() {
        eprintln!(
            "Warning: No data to write to CSV for NACA {}. CSV will contain headers only.",
            naca
        );
    }
    for (aoa, cl_opt) in data {
        let cl_str = match cl_opt {
            Some(cl_val) => format!("{:.4}", cl_val),
            None => "N/A".to_string(),
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
    let cli = Cli::parse(); // 获取全局参数
    let polar_path = &cli.polar_path; // 使用全局 polar_path

    if !args.no_delete {
        if Path::new(polar_path).exists() {
            if let Err(e) = fs::remove_dir_all(polar_path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    return Err(format!(
                        "Failed to delete existing polar directory '{}': {}",
                        polar_path, e
                    )
                    .into());
                }
            }
        }
    }
    fs::create_dir_all(polar_path)?;

    println!(
        "Analyzing NACA {} at Re = {} from AoA {:.1}° to {:.1}° (step {:.2}°)...",
        args.naca, args.reynolds, args.min_aoa, args.max_aoa, args.aoa_step
    );

    let num_steps = (((args.max_aoa - args.min_aoa) / args.aoa_step).floor() as u64 + 1).max(1);
    let pb = setup_progress_bar(num_steps, "Sweeping AoA");
    pb.set_length(num_steps);

    match perform_xfoil_sweep(xfoil_path, args, &pb) {
        Ok(analysis_result) => {
            pb.finish_with_message("Sweep analysis complete.");
            display_analysis_summary(args, &analysis_result);
        }
        Err(e) => {
            pb.finish_with_message(format!("Sweep analysis failed: {}", e));
            eprintln!("\nSweep analysis concluded with an error: {}", e);
            println!(
                "\nNo suitable aerodynamic performance data found due to an error or lack of valid convergence points."
            );
        }
    }
    Ok(())
}

fn handle_get_cl_command(
    xfoil_path: &str,
    args: &GetClArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "Calculating Cl for NACA {} at Re = {} from AoA {:.2}° to {:.2}° (step {:.2}°)...",
        args.naca, args.reynolds, args.min_aoa, args.max_aoa, args.aoa_step
    );

    
    unimplemented!()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_and_validate_cli()?;

    match &cli.command {
        Commands::Sweep(args) => handle_sweep_command(&cli.xfoil_path, args)?,
        Commands::GetCl(args) => handle_get_cl_command(&cli.xfoil_path, args)?,
    }

    Ok(())
}
