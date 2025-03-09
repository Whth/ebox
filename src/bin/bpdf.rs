use clap::Parser;
use std::fs::{self, create_dir_all};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

/// Batch PDF processor using magic-pdf external command
#[derive(Parser)]
#[command(author, version)]
struct Cli {
    /// Path to a PDF file or directory containing PDFs
    #[arg(short, long)]
    path: PathBuf,

    /// Output directory for processed files
    #[arg(short, long, default_value = "./output")]
    output: PathBuf,

    /// Maximum number of files per chunk
    #[arg(short, long, default_value_t = 10)]
    chunk_size: usize,

    /// Enable verbose logging
    #[arg(short, long, action)]
    verbose: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    run(cli)
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    // Ensure output directory exists
    create_dir_all(&cli.output)?;

    // Collect all PDF files
    let pdf_files = collect_pdf_files(&cli)?;

    if pdf_files.is_empty() {
        println!("No PDF files found in the specified path.");
        return Ok(());
    }

    log_verbose(
        &cli,
        &format!("Found {} PDF files to process", pdf_files.len()),
    );

    // Create chunks
    let chunks = chunk_files(pdf_files, cli.chunk_size);

    // Process each chunk
    process_chunks(&chunks, &cli)?;

    println!(
        "All PDF processing complete. Results are in {:?}",
        cli.output
    );
    Ok(())
}

fn collect_pdf_files(cli: &Cli) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    if cli.path.is_file() {
        if cli.path.extension().map_or(false, |ext| ext == "pdf") {
            Ok(vec![cli.path.clone()])
        } else {
            Err("Specified file is not a PDF".into())
        }
    } else {
        Ok(find_pdf_files(&cli.path))
    }
}

fn process_chunks(chunks: &[Vec<PathBuf>], cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    for (i, chunk) in chunks.iter().enumerate() {
        let chunk_dir = cli.output.join(format!("chunk_{}", i + 1));
        create_dir_all(&chunk_dir)?;

        log_verbose(
            cli,
            &format!("Processing chunk {} with {} files", i + 1, chunk.len()),
        );

        // Copy files to chunk directory
        copy_files_to_chunk(chunk, &chunk_dir, cli)?;

        // Process the chunk with magic-pdf
        process_chunk(&chunk_dir, &cli.output, cli.verbose)?;
    }
    Ok(())
}

fn copy_files_to_chunk(
    files: &[PathBuf],
    chunk_dir: &Path,
    cli: &Cli,
) -> Result<(), Box<dyn std::error::Error>> {
    for pdf in files {
        let filename = pdf.file_name().unwrap();
        let destination = chunk_dir.join(filename);
        fs::copy(pdf, &destination)?;

        log_verbose(cli, &format!("Copied {:?} to {:?}", pdf, destination));
    }
    Ok(())
}

fn log_verbose(cli: &Cli, message: &str) {
    if cli.verbose {
        println!("{}", message);
    }
}

fn find_pdf_files(dir: &Path) -> Vec<PathBuf> {
    WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| !e.file_type().is_dir())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "pdf"))
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn chunk_files(files: Vec<PathBuf>, chunk_size: usize) -> Vec<Vec<PathBuf>> {
    files
        .chunks(chunk_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

fn process_chunk(
    chunk_dir: &Path,
    output_dir: &Path,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if verbose {
        println!("Processing directory {:?} with magic-pdf", chunk_dir);
    }

    let result = Command::new("magic-pdf")
        .arg("-p")
        .arg(chunk_dir)
        .arg("-o")
        .arg(output_dir)
        .output()?;

    if verbose {
        println!("magic-pdf exit status: {}", result.status);
        if !result.stdout.is_empty() {
            println!("Output: {}", String::from_utf8_lossy(&result.stdout));
        }
        if !result.stderr.is_empty() {
            eprintln!("Error: {}", String::from_utf8_lossy(&result.stderr));
        }
    }

    if !result.status.success() {
        eprintln!("magic-pdf command failed on chunk {:?}", chunk_dir);
    }

    Ok(())
}
