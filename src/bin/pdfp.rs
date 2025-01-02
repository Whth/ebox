use clap::{Parser, Subcommand};
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{fs, io};

/// CLI structure for parsing command-line arguments.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Subcommands for the CLI tool.
#[derive(Subcommand)]
enum Commands {
    /// Extract images from PDFs in a directory
    #[command(visible_alias = "E")]
    Extract {
        /// Path to the input directory containing PDF files
        input_dir: PathBuf,

        /// Output directory for extracted images
        #[arg(default_value = "./pdf-images")]
        out_dir: PathBuf,

        /// Minimum file size (in kilobytes) to keep; files smaller than this will be deleted
        #[arg(long, short)]
        min_size: Option<u64>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Extract { input_dir, out_dir, min_size } => {
            // Ensure output directory exists
            fs::create_dir_all(out_dir).expect("Failed to create output directory");

            // Process each PDF file in the input directory
            process_pdfs(input_dir, out_dir).expect("Failed to process PDFs");

            // Remove files smaller than the specified minimum size
            if let Some(min_size_value) = min_size {
                remove_small_files(out_dir, *min_size_value).expect("Failed to remove small files");
            }
        }
    }

    Ok(())
}

/// Processes each PDF file in the input directory by extracting images.
///
/// # Arguments
///
/// * `input_dir` - A reference to the path of the input directory containing PDF files.
/// * `out_dir` - A reference to the path of the output directory for extracted images.
///
/// # Returns
///
/// * `Result<(), Box<dyn std::error::Error>>` - An empty result on success, or an error on failure.
fn process_pdfs(
    input_dir: &PathBuf,
    out_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::read_dir(input_dir)?
        .par_bridge()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file() &&
            entry.path()
                .extension()
                .is_some_and(|s| s.to_ascii_lowercase().to_str() == Some("pdf")))
        .inspect(|entry| println!("Processing {}", entry.path().display()))
        .for_each(|entry| {
            let path = entry.path();
            let pdf_name = path.file_stem().unwrap().to_str().unwrap();
            let pdf_output_dir = out_dir.join(pdf_name);

            // Create subdirectory for this PDF's images
            fs::create_dir_all(&pdf_output_dir).expect("Failed to create output directory");

            // Step 1: Extract images from PDF using pdfimages
            extract_images_from_pdf(&path, &pdf_output_dir).expect("Failed to execute pdfimages");
        });

    Ok(())
}

/// Extracts images from a PDF file using the `pdfimages` command.
///
/// # Arguments
///
/// * `pdf_path` - A reference to the path of the PDF file.
/// * `output_dir` - A reference to the path of the output directory for extracted images.
///
/// # Returns
///
/// * `Result<(), io::Error>` - An empty result on success, or an I/O error on failure.
fn extract_images_from_pdf(pdf_path: &PathBuf, output_dir: &PathBuf) -> io::Result<()> {
    Command::new("pdfimages")
        .arg("-png") // Extract as PNG
        .arg("-p") // Preserve aspect ratio
        .arg(pdf_path)
        .arg(format!("{}/", output_dir.display()))
        .output()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("Failed to execute pdfimages: {}", e)))?;

    Ok(())
}

/// Removes files smaller than the specified minimum size in the output directory.
///
/// # Arguments
///
/// * `out_dir` - A reference to the path of the output directory.
/// * `min_size` - The minimum file size (in kilobytes) to keep.
///
/// # Returns
///
/// * `Result<(), io::Error>` - An empty result on success, or an I/O error on failure.
fn remove_small_files(out_dir: &PathBuf, min_size: u64) -> io::Result<()> {
    fs::read_dir(out_dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .flat_map(|dir_entry| fs::read_dir(dir_entry.path()).expect("Failed to read directory"))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .for_each(|entry| {
            let file_path = entry.path();
            if let Ok(metadata) = fs::metadata(&file_path) {
                if metadata.len() < (min_size * 1024) {
                    fs::remove_file(&file_path).expect("Failed to delete small file");
                }
            }
        });

    Ok(())
}