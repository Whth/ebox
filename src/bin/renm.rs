use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// A Tool that renames files in a directory and manages mappings.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The directory to process files in
    directory: String,

    /// The output mapping file name (used in rename mode)
    #[arg(
        short,
        long,
        default_value = "rename_map.json",
        conflicts_with = "restore"
    )]
    output: String,

    /// Restore filenames using the specified mapping file
    #[arg(long, conflicts_with = "output")]
    restore: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if let Some(restore_map) = args.restore {
        restore_files(&args.directory, &restore_map)?;
    } else {
        rename_files(&args.directory, &args.output)?;
    }

    Ok(())
}

fn rename_files(directory: &str, output: &str) -> Result<(), Box<dyn std::error::Error>> {
    let entries = fs::read_dir(directory)?;
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .collect();

    files.sort_by(|a, b| a.file_name().to_str().cmp(&b.file_name().to_str()));

    let mut rename_ops = Vec::new();
    for (i, entry) in files.iter().enumerate() {
        let old_name = entry.file_name().to_str().unwrap().to_string();

        // 修复后的extension获取方式
        let extension: String = entry
            .path()
            .extension()
            .and_then(|s| s.to_str().map(|s| s.to_owned()))
            .unwrap_or_default();

        let new_name = if extension.is_empty() {
            format!("{}", i + 1)
        } else {
            format!("{}.{}", i + 1, extension)
        };

        let new_path = entry.path().with_file_name(&new_name);
        rename_ops.push((entry.path().to_path_buf(), new_path, old_name));
    }

    check_conflicts(&rename_ops)?;
    perform_renames(&rename_ops)?;
    save_mapping(&rename_ops, output)?;
    Ok(())
}

// 其他函数保持不变...

fn restore_files(directory: &str, restore_map: &str) -> Result<(), Box<dyn std::error::Error>> {
    let json = fs::read_to_string(restore_map)?;
    let mapping: HashMap<String, String> = serde_json::from_str(&json)?;

    let dir = Path::new(directory);
    let mut restore_ops = Vec::new();
    let mut missing = Vec::new();

    for (old_name, new_name) in &mapping {
        let new_path = dir.join(new_name);
        let old_path = dir.join(old_name);

        if new_path.exists() {
            restore_ops.push((new_path.clone(), old_path.clone()));
        } else {
            missing.push(new_name.clone());
        }
    }

    if !missing.is_empty() {
        eprintln!("Warning: Missing files to restore:");
        for name in &missing {
            eprintln!("- {}", name);
        }
    }

    check_restore_conflicts(&restore_ops)?;
    perform_restore(&restore_ops)?;
    println!(
        "\nRestored {} files using {}",
        restore_ops.len(),
        restore_map
    );
    Ok(())
}

fn perform_restore(
    restore_ops: &[(std::path::PathBuf, std::path::PathBuf)],
) -> Result<(), Box<dyn std::error::Error>> {
    let pb = ProgressBar::new(restore_ops.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.yellow} [{elapsed_precise}] [{bar:40.yellow/green}] {pos}/{len} ({eta}) Restoring...")?
        .progress_chars("##-"));

    for (new_path, old_path) in restore_ops {
        pb.inc(1);
        fs::rename(new_path, old_path)?;
    }
    pb.finish_with_message("Restoration completed");
    Ok(())
}

fn check_conflicts(
    rename_ops: &[(std::path::PathBuf, std::path::PathBuf, String)],
) -> Result<(), Box<dyn std::error::Error>> {
    let pb = ProgressBar::new(rename_ops.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) Checking...")?
        .progress_chars("##-"));

    for (_, new_path, _) in rename_ops {
        pb.inc(1);
        if new_path.exists() {
            pb.finish_with_message("Conflict detected");
            eprintln!("\nError: File already exists: {}", new_path.display());
            std::process::exit(1);
        }
    }
    pb.finish_with_message("No conflicts found");
    Ok(())
}

fn perform_renames(
    rename_ops: &[(std::path::PathBuf, std::path::PathBuf, String)],
) -> Result<(), Box<dyn std::error::Error>> {
    let pb = ProgressBar::new(rename_ops.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.yellow} [{elapsed_precise}] [{bar:40.yellow/green}] {pos}/{len} ({eta}) Renaming...")?
        .progress_chars("##-"));

    for (old_path, new_path, _) in rename_ops {
        pb.inc(1);
        fs::rename(old_path, new_path)?;
    }
    pb.finish_with_message("Renaming completed");
    Ok(())
}

fn save_mapping(
    rename_ops: &[(std::path::PathBuf, std::path::PathBuf, String)],
    output: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mapping: HashMap<String, String> = rename_ops
        .iter()
        .map(|(_, new_path, old_name)| {
            (
                old_name.clone(),
                new_path.file_name().unwrap().to_str().unwrap().to_string(),
            )
        })
        .collect();

    let json = serde_json::to_string_pretty(&mapping)?;
    fs::write(output, json)?;
    println!("\nSaved mapping to {}", output);
    Ok(())
}

fn check_restore_conflicts(
    restore_ops: &[(std::path::PathBuf, std::path::PathBuf)],
) -> Result<(), Box<dyn std::error::Error>> {
    let pb = ProgressBar::new(restore_ops.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) Checking...")?
        .progress_chars("##-"));

    for (new_path, old_path) in restore_ops {
        pb.inc(1);
        if old_path.exists() {
            pb.finish_with_message("Conflict detected");
            eprintln!("\nError: File already exists: {}", old_path.display());
            std::process::exit(1);
        }

        if new_path.exists() {
            pb.finish_with_message("Conflict detected");
            eprintln!("\nError: File already exists: {}", new_path.display());
            std::process::exit(1);
        }
    }
    pb.finish_with_message("No conflicts found");
    Ok(())
}
