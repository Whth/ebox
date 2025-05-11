use crate::{Cli, Commands, SweepArgs};
use clap::Parser;
use foxil::result::AnalysisResult;
use std::path::Path;

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

pub fn display_analysis_summary(args: &SweepArgs, result: &AnalysisResult) {
    println!("\n--- Optimal Aerodynamic Performance (Sweep) ---");
    println!("Airfoil: NACA {}", args.naca);
    println!("Reynolds Number: {}", args.reynolds);
    println!("Best Angle of Attack (for max Cl/Cd): {:.2}°", result.aoa);
    println!("Lift Coefficient (Cl) at best AoA: {:.4}", result.cl);
    println!("Drag Coefficient (Cd) at best AoA: {:.4}", result.cd);
    println!("Maximum Cl/Cd Ratio: {:.4}", result.ld_ratio);
}
