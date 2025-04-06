use clap::Parser;
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json;
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

    /// Ignore extensions and use pure numeric filenames
    #[arg(long)]
    ignore_extension: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if let Some(restore_map) = args.restore.clone() {
        restore_files(&args.directory, &restore_map, args.ignore_extension)?;
    } else {
        rename_files(&args.directory, &args.output, args.ignore_extension)?;
    }

    Ok(())
}

fn rename_files(
    directory: &str,
    output: &str,
    ignore_extension: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = fs::read_dir(directory)?;
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .collect();

    files.sort_by(|a, b| a.file_name().to_str().cmp(&b.file_name().to_str()));

    let mut rename_ops = Vec::new();
    for (i, entry) in files.iter().enumerate() {
        let old_name = entry.file_name().to_str().unwrap().to_string();
        let extension: String = entry
            .path()
            .extension()
            .and_then(|s| s.to_str().map(|s| s.to_owned()))
            .unwrap_or_default();

        let new_name = if ignore_extension {
            (i + 1).to_string()
        } else {
            if extension.is_empty() {
                (i + 1).to_string()
            } else {
                format!("{}.{}", i + 1, extension)
            }
        };

        let new_path = entry.path().with_file_name(&new_name);
        rename_ops.push((entry.path().to_path_buf(), new_path, old_name));
    }

    check_conflicts(&rename_ops)?;
    perform_renames(&rename_ops)?;
    save_mapping(&rename_ops, output)?;
    Ok(())
}

fn restore_files(
    directory: &str,
    restore_map: &str,
    ignore_extension: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = fs::read_to_string(restore_map)?;
    let mapping: HashMap<String, String> = serde_json::from_str(&json)?;

    let dir = Path::new(directory);
    let (restore_ops, missing): (Vec<_>, Vec<_>) = mapping
        .iter()
        .map(|(old_name, new_name)| {
            process_mapping_entry(dir, old_name, new_name, ignore_extension)
        })
        .partition(Result::is_ok);

    let restore_ops: Vec<_> = restore_ops.into_iter().map(Result::unwrap).collect();
    let missing: Vec<_> = missing.into_iter().filter_map(Result::err).collect();

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

fn process_mapping_entry(
    dir: &Path,
    old_name: &str,
    new_name: &str,
    ignore_extension: bool,
) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let new_path = dir.join(new_name);
    let old_path = dir.join(old_name);

    if ignore_extension {
        find_file_by_patterns(dir, new_name)
            .map(|found_path| (found_path, old_path))
            .ok_or_else(|| new_name.to_string())
    } else {
        if new_path.exists() {
            Ok((new_path, old_path))
        } else {
            Err(new_name.to_string())
        }
    }
}

fn find_file_by_patterns(dir: &Path, new_name: &str) -> Option<std::path::PathBuf> {
    let patterns = &[
        format!("{}", new_name),   // Pure numeric
        format!("{}.*", new_name), // With any extension
    ];

    patterns.iter().find_map(|pattern| {
        glob(&dir.join(pattern).to_str().unwrap())
            .ok()?
            .filter_map(Result::ok)
            .find(|path| path.is_file())
    })
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
