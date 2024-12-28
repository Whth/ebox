use clap::Parser;
use rayon::prelude::*;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};


/// Strips the last occurrence of a delimiter from each line in a text file.
#[derive(Parser)]
struct Cli {
    /// The input directory containing the text files.
    #[arg(default_value = ".")]
    input_dir: PathBuf,

    /// The file extension of the text files to process.
    #[arg(short, long, default_value = "txt")]
    extension: String,

    /// The output directory where the stripped text files will be saved.
    #[arg(short, long, default_value = "./striped")]
    output_dir: PathBuf,

    /// The delimiter to strip from each line.
    #[arg(short, long, default_value = "//")]
    delimiter: String,
}

fn process_file(input_path: &Path, output_path: &Path, delimiter: &str) -> io::Result<()> {
    let file = File::open(input_path).expect("Failed to open file");
    let reader = BufReader::new(file);
    let mut output_file = File::create(output_path)?;

    for line in reader.lines() {
        let line = line?;
        if let Some(pos) = line.find(delimiter) {
            writeln!(output_file, "{}", &line[..pos])?;
            return Ok(());
        } else {
            writeln!(output_file, "{}", line)?;
        }
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let args = Cli::parse();

    if !args.output_dir.exists() {
        fs::create_dir_all(&args.output_dir)?;
    }

    let processed = fs::read_dir(args.input_dir)
        .expect("Failed to read input directory")
        .par_bridge()
        .filter_map(|entry| entry.ok())
        .inspect(|entry| println!("Processing {}", entry.path().display()))
        .map(|entry| {
            let path = entry.path();
            if path.is_file() && path.extension().unwrap_or_default().to_string_lossy() == args.extension {
                let output_path = args.output_dir.join(path.file_name().unwrap());
                process_file(&path, &output_path, &args.delimiter).expect("Failed to process file");
            }
        })
        .count();

    println!("Processed {} files", processed);
    Ok(())
}



