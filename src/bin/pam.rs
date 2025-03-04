use clap::{Parser, Subcommand};
use indicatif::{ParallelProgressIterator, ProgressFinish, ProgressStyle};
use rayon::prelude::*;
use std::borrow::Cow;
use std::fs;
use std::fs::read_dir;
use std::io;
use std::path::{Path, PathBuf};

/// A simple tool to merge image and video files into a single directory.
#[derive(Parser)]
#[command(name = "Picture Assembler", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Audit subdirectories within INPUT_DIR for those with fewer than MIN_COUNT images.
    Audit {
        /// Minimum number of images required in a subdirectory.
        #[arg(short, long, default_value = "5")]
        min_count: u32,
        /// Input directory to audit.
        input_dir: PathBuf,
    },
    /// Eradicate all non-image files in the target directories.
    Eradicate {
        /// Output directory for the eradicated files.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Input directories to process.
        input_dirs: Vec<PathBuf>,
    },
    /// Merge image and video files into a single directory.
    Merge {
        /// Whether to cut files during the merge process.
        #[arg(short, long)]
        cut: bool,
        /// Enable verbose output.
        #[arg(short, long)]
        verbose: bool,
        /// Output directory for merged files.
        #[arg(short, long, default_value = "./merged")]
        output: PathBuf,
        /// Input directories to process.
        input_dirs: Vec<PathBuf>,
    },
}


fn glob_dir(p: &PathBuf) -> Vec<PathBuf> {
    glob::glob(p.to_str().unwrap())
        .map_err(
            |e| (e, format!("Failed to read directory {:?}", p))
        )
        .ok()
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.is_dir())
        .collect()
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Audit { min_count, input_dir } => audit(input_dir, *min_count),
        Commands::Eradicate { output, input_dirs } => {
            eradicate(output, input_dirs.iter().flat_map(glob_dir).collect::<Vec<PathBuf>>().as_slice())
        }
        Commands::Merge { cut, verbose, output, input_dirs } => {
            merge(*cut, *verbose, output, input_dirs
                .iter().flat_map(glob_dir).collect::<Vec<PathBuf>>().as_slice())
        }
    }
}

/// Check subdirectories within INPUT_DIR for those with fewer than MIN_COUNT images.
fn audit(input_dir: &Path, min_count: u32) -> io::Result<()> {
    if !input_dir.is_dir() {
        return Err(io::Error::new(io::ErrorKind::Other, format!("Input directory {:?} does not exist or is not a directory.", input_dir)));
    }

    read_dir(input_dir)
        .expect("Failed to read directory")
        .par_bridge()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .for_each(|entry| {
            let path = entry.path();
            let file_count = fs::read_dir(&path)
                .expect("Failed to read directory")
                .filter_map(|e| e.ok())
                .filter(|e| is_image_or_video(&e.path()))
                .count();
            if file_count < min_count as usize {
                println!("{:?} has fewer than {} images/video files (found {}).", path, min_count, file_count);
            }
        });

    Ok(())
}

/// Delete all non-image files in the target directories.
fn eradicate(output: &Option<PathBuf>, input_dirs: &[PathBuf]) -> io::Result<()> {
    get_multimedia(input_dirs)
        .iter()
        .par_bridge()
        .try_for_each(|path| {
            if let Some(output_dir) = output {
                // Create the output directory if it doesn't exist
                fs::create_dir_all(output_dir)?;
                // Move the file to the output directory
                let new_path = output_dir.join(path.file_name().unwrap());
                if fs::rename(path, &new_path).is_err() {
                    fs::copy(path, &new_path)?;
                    fs::remove_file(path)?;
                }
            } else {
                // Delete the file if no output directory is specified
                fs::remove_file(path)?;
            }
            Ok::<(), io::Error>(())
        })?;

    Ok(())
}

fn merge(cut: bool, verbose: bool, output: &PathBuf, input_dirs: &[PathBuf]) -> io::Result<()> {
    fs::create_dir_all(output)?;
    println!("Merging merge {:?} into {:?}", input_dirs, output);

    let files = get_multimedia(input_dirs);
    println!("💡 Found {} files", files.len());
    files
        .iter()
        .par_bridge()
        .progress_count(files.len() as u64)
        .with_finish(ProgressFinish::WithMessage(Cow::from("Done")))
        .with_style(ProgressStyle::with_template("{spinner:.green} [{elapsed}/{duration}] [{bar:40.green/blue}] {msg} {pos}/{len} ({per_sec})").unwrap())
        .map(|path| (path.clone(), path
            .parent()
            .expect("Failed to get parent directory")
            .file_name()
            .expect("Failed to get file name")
            .to_string_lossy()
            .split('_')
            .last()
            .expect("Failed to get artist ID")
            .to_owned()
        ))
        .map(|(p, artist_id)| (p.clone(), read_dir(output)
            .expect("Failed to read directory")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains(artist_id.as_str()))
            .last()
            .map(|entry| entry.path())
            .unwrap_or(output.join(
                p
                    .parent()
                    .expect("Failed to get parent directory")
                    .file_name()
                    .expect("Failed to get file name"))
            ))
        )
        .filter(|(src, out)| should_process_file(src, &out, verbose))
        .try_for_each(|(path, out_dir)| {
            let new_path = out_dir.join(path.file_name().unwrap());
            fs::create_dir_all(&out_dir)?;
            if cut {
                if let Err(_) = fs::rename(&path, &new_path) {
                    fs::copy(&path, &new_path).expect("Failed to rename file");
                    fs::remove_file(&path).expect("Failed to remove file");
                }
            } else {
                fs::copy(&path, &new_path).expect("Failed to copy file");
            }
            if verbose {
                println!("Processed {:?}", path);
            }
            Ok(())
        })
}

/// Function that determines whether a file should be processed based on size comparison.
fn should_process_file(source: &Path, output: &PathBuf, verbose: bool) -> bool {
    let source_metadata = match fs::metadata(source) {
        Ok(meta) => meta,
        Err(_) => return true, // If we can't get metadata, process the file.
    };

    let target_path = output.join(source.file_name().unwrap_or_else(|| "".as_ref()));

    if target_path.exists() {
        let target_metadata = match fs::metadata(&target_path) {
            Ok(meta) => meta,
            Err(_) => return true, // If we can't get metadata of target, process the file.
        };

        if target_metadata.len() >= source_metadata.len() {
            if verbose {
                println!("Skipped {:?} due to existing file with equal or greater size", source);
            }
            return false; // Skip this file.
        }
    }

    true // Process the file.
}

/// Check if a file is an image or video.
fn is_image_or_video(path: &PathBuf) -> bool {
    matches!(path.extension().and_then(|s| s.to_str()), Some("jpg" | "jpeg" | "png" | "gif" | "mp4" | "avi" | "mov"))
}

fn get_multimedia(dir_path: &[PathBuf]) -> Vec<PathBuf> {
    dir_path
        .iter()
        .par_bridge()
        .progress_count(dir_path.len() as u64)
        .filter(|dir| dir.is_dir())
        .filter_map(|dir| dir.read_dir().ok())
        .flatten_iter()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter_map(|entry| entry.is_dir().then_some(read_dir(entry)))
        .filter_map(|dir| dir.ok())
        .flatten_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| is_image_or_video(&entry.path()))
        .map(|entry| entry.path())
        .collect()
}