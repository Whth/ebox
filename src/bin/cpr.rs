use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Move files from a target directory to a reference directory, ensuring that only files that exist in the reference directory are moved.
#[derive(Parser)]
struct Cli {
    /// Target directory A
    target_dir: String,
    /// Reference directory B
    reference_dir: String,
}

/// Recursively retrieves all file paths in a given directory.
///
/// # Arguments
///
/// * `dir` - A string slice that holds the path to the directory.
///
/// # Returns
///
/// A vector containing all file paths found within the directory.
fn get_files(dir: &str) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .min_depth(1)
        .into_iter()
        .filter_map(|entry| entry.ok().map(|e| e.into_path()))
        .filter(|path| path.is_file())
        .collect()
}

/// Main function that orchestrates the synchronization process.
///
/// It reads files from both directories, checks for matching files in the reference directory,
/// and copies them to the target directory if they exist.
///
/// This version uses Rayon for parallel processing of copying operations.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let target_dir = Path::new(&cli.target_dir);
    let reference_dir = Path::new(&cli.reference_dir);

    // Retrieve all file paths in the target and reference directories
    let target_files = get_files(target_dir.to_str().unwrap());
    let reference_files_set: std::collections::HashSet<_> =
        get_files(reference_dir.to_str().unwrap()).iter().cloned().collect();

    // Find files to copy by checking if the corresponding file exists in the reference directory
    let files_to_copy: Vec<_> = target_files.iter()
        .filter(|target_file| {
            if let Some(relative_path) = target_file.strip_prefix(target_dir).ok() {
                let reference_file = reference_dir.join(relative_path);
                reference_files_set.contains(&reference_file)
            } else {
                false
            }
        })
        .map(|target_file| {
            let relative_path = target_file.strip_prefix(target_dir).unwrap();
            let reference_file = reference_dir.join(relative_path);
            (reference_file, target_file.clone())
        })
        .collect();

    // Create a progress bar
    let pb = ProgressBar::new(files_to_copy.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed}/{duration}] [{bar:40.green/blue}] {msg} {pos}/{len} ({per_sec})")
            .unwrap()
            .progress_chars("#>-"),
    );

    // Process each file for copying in parallel using Rayon
    files_to_copy.par_iter().for_each(|(src, dst)| {
        fs::copy(src, dst).expect("Failed to copy file");
        pb.inc(1);
    });

    pb.finish_with_message("Done!");
    Ok(())
}