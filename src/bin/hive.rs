/// A CLI tool to parse citation patterns like `#cite()` in a file and generate statistics.
///
/// This tool reads a file specified by the user, searches for all occurrences of the `#cite(<citation>)` pattern,
/// and outputs the frequency of each citation found in the document.
use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;


#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
/// A CLI tool to parse citation patterns like `#cite()` in a file and generate statistics.
struct Cli {
    #[clap(value_parser)]
    /// The path to the file to parse.
    file_path: PathBuf,
}

fn main() {
    let args = Cli::parse();
    let content = fs::read_to_string(&args.file_path).expect("Failed to read file");

    let re = Regex::new(r#"#cite\(([^)]+)\)"#).unwrap();
    let mut stats: HashMap<String, usize> = HashMap::new();

    for cap in re.captures_iter(&content) {
        if let Some(matched) = cap.get(1) {
            let key = matched.as_str().to_string();
            *stats.entry(key).or_insert(0) += 1;
        }
    }

    println!("Statistics for citations:");
    for (citation, count) in stats {
        println!("{}: {}", citation, count);
    }
}
