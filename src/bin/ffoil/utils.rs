use crate::{Cli, Commands, SweepArgs};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
pub struct XfoilResult {
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
    pub(crate) fn get_analysis_result(&self, aoa: f64) -> AnalysisResult {
        if let Some(idx) = self.alpha.iter().position(|&x| x == aoa) {
            let cl = self.cl.get(idx).copied().expect("cl not found!");
            let cd = self.cd.get(idx).copied().expect("cd not found!");
            AnalysisResult::valid_result(aoa, cl, cd)
        } else {
            AnalysisResult::default()
        }
    }
}

#[derive(Default)]
pub struct AnalysisResult {
    pub(crate) aoa: f64,
    pub(crate) cl: f64,
    pub(crate) cd: f64,
    pub(crate) ld_ratio: f64,
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

    pub(crate) fn is_valid(&self) -> bool {
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

pub fn parse_and_validate_cli() -> Result<Cli, Box<dyn std::error::Error>> {
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

pub fn setup_progress_bar(num_steps: u64, description: &str) -> ProgressBar {
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

pub fn display_analysis_summary(args: &SweepArgs, result: &AnalysisResult) {
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
