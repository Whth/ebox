use clap::Parser;
use std::fs::{self, File};
use std::io::{self, BufRead};
use walkdir::WalkDir;

#[derive(Parser)]
struct Args {
    /// The directory containing the txt files
    dir: String,

    /// The delimiter used to split the strings in the txt files
    #[arg(short, long, default_value = "最终概述：")]
    delimiter: String,

    /// The output file path
    #[arg(short, long, default_value = "output.txt")]
    output: String,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // Collect all .txt files in the directory
    let txt_files: Vec<String> = WalkDir::new(&args.dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "txt"))
        .map(|e| e.into_path().to_string_lossy().to_string())
        .collect();

    // Process each file and collect the last segment of each line
    let mut segments: Vec<String> = Vec::new();
    for file_path in txt_files {
        let file = File::open(&file_path)?;
        let reader = io::BufReader::new(file);
        let all_data = reader.lines().map_while(Result::ok).collect::<String>();

        let segs = all_data.split(&args.delimiter).collect::<Vec<_>>();
        if segs.len() < 2 {
            println!("{} has no delimiter", file_path);
            continue;
        }

        if let Some(last_segment) = all_data.split(&args.delimiter).last() {
            if last_segment.is_empty() {
                println!("{} has empty last segment", file_path);
                continue;
            }
            segments.push(last_segment.to_string());
        }
    }

    // Join all segments with ";"
    let result = segments.join(";");

    // Write the result to the output file
    fs::write(&args.output, result)?;

    Ok(())
}
