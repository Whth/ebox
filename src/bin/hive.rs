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

    #[clap(short, long, default_value_t = 0)]
    /// The minimum occurrence threshold for citations to be included in the output.
    threshold: usize,
}

fn main() {
    let args = Cli::parse();
    let content = fs::read_to_string(&args.file_path).expect("Failed to read file");
    let content_length = content.len() as f64; // Total length of the file for density calculation

    let re = Regex::new(r#"#cite\(([^)]+)\)"#).unwrap();
    let mut stats: HashMap<String, (usize, Vec<usize>)> = HashMap::new(); // (count, positions)

    for (index, cap) in re.captures_iter(&content).enumerate() {
        if let Some(matched) = cap.get(1) {
            let key = matched.as_str().to_string();
            let entry = stats.entry(key).or_insert((0, Vec::new()));
            entry.0 += 1; // Increment count
            entry.1.push(index); // Record position
        }
    }

    // Sort the statistics by count in descending order
    let mut sorted_stats: Vec<_> = stats.into_iter().collect();
    sorted_stats.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));

    println!("Statistics for citations (threshold: {}):", args.threshold);
    for (citation, (count, positions)) in sorted_stats {
        if count >= args.threshold {
            let average_position = positions.iter().sum::<usize>() as f64 / positions.len() as f64;
            let density = average_position / content_length; // Calculate density
            println!("{}: count={}, density={:.4}", citation, count, density);
        }
    }
}