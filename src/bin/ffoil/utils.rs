use crate::SweepArgs;
use foxil::result::AnalysisResult;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};

fn validate_xfoil_path(xfoil_path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    if !Path::new(xfoil_path).exists() {
        eprintln!(
            "Error: XFoil executable not found at '{}'.",
            xfoil_path.display()
        );
        eprintln!("Please specify the correct path using --xfoil-path, the XFOIL_PATH environment variable, or ensure it's in the default location.");
        return Err(format!("XFoil executable not found: {}", xfoil_path.display()).into());
    }
    Ok(())
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
pub fn setup_progress_bar(num_steps: u64, description: &str) -> ProgressBar {
    let pb = ProgressBar::new(num_steps);
    let template = format!("{{spinner:.green}} [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {{pos}}/{{len}} ({{eta}}) {}: {{msg}}", description);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(&template)
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("██-"),
    );
    pb
}
