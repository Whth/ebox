use args_parser::Intervals;
use chrono::Duration;
use clap::Parser;
use clap_verbosity_flag::Verbosity;
use colored::Colorize;
use csv::{ReaderBuilder, StringRecord};
use num_cpus::get;
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use std::fs::{create_dir_all, File};
use std::io::BufReader;
use std::path::PathBuf;
mod args_parser;
mod tests;

trait AsDuration {
    fn as_duration(&self) -> Option<Duration>;
}

impl AsDuration for str {
    fn as_duration(&self) -> Option<Duration> {
        let long_reg = Regex::new(r"(\d{2}):(\d{2}):(\d{2})").unwrap();
        match long_reg.captures(self) {
            None => {}
            Some(cap) => {
                let hours = cap.get(1).unwrap().as_str().parse::<i64>().unwrap();
                let minutes = cap.get(2).unwrap().as_str().parse::<i64>().unwrap();
                let seconds = cap.get(3).unwrap().as_str().parse::<i64>().unwrap();
                return Some(Duration::seconds(hours * 3600 + minutes * 60 + seconds));
            }
        }

        let short_reg = Regex::new(r"(\d{2}):(\d{2})").unwrap();
        match short_reg.captures(self) {
            None => {
                None
            }
            Some(cap) => {
                let minutes = cap.get(1).unwrap().as_str().parse::<i64>().unwrap();
                let seconds = cap.get(2).unwrap().as_str().parse::<i64>().unwrap();
                Some(Duration::seconds(minutes * 60 + seconds))
            }
        }
    }
}


/// A Cli app used to classify the video according the given criteria
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// The path to the csv video file
    #[arg(short, long, )]
    file: PathBuf,

    /// The path to the output directory
    #[arg(short, long, default_value = "./classified")]
    output_dir: PathBuf,

    /// The number of threads to use
    #[arg(short, long,default_value_t=get())]
    threads: usize,

    /// Intervals separated by commas, each interval is a pair of numbers separated by a colon
    #[arg(short, long, default_value = "0:60,60:180,180:360,360:720,720:2880")]
    intervals: Intervals,

    /// The label to use for the video length
    #[arg(short, long, default_value = "length")]
    length_label: String,

    /// Verbosity flags
    #[command(flatten)]
    verbose: Verbosity,

}


fn main() {
    let args = Args::parse();
    rayon::ThreadPoolBuilder::new().num_threads(args.threads).build_global().unwrap();

    let file = File::open(&args.file).expect("Failed to open file");
    let buf_reader = BufReader::new(file);
    let mut reader =
        ReaderBuilder::new()
            .delimiter(b',')
            .quoting(false)
            .flexible(false)
            .has_headers(true)
            .from_reader(buf_reader);


    let header = reader.headers().unwrap().clone();
    let pos = match header.iter().position(|s| { s == args.length_label })
    {
        Some(pos) => pos,
        None => {
            println!("{}", format!("{} is not find in {}", args.length_label, args.file.display()).red());
            return;
        }
    };

    let mut map: HashMap<String, Vec<StringRecord>> = HashMap::new();
    for record in reader.records().filter_map(|r| r.ok()) {
        let duration = if let Some(du) = record[pos].as_duration() {
            du
        } else {
            println!("{} is not a valid duration", &record[pos]);
            continue;
        };

        let key = if let Some(interval) =
            args.intervals.in_which_interval(duration.num_seconds()) {
            interval.to_string()
        } else {
            "other".to_string()
        };

        map.entry(key).or_default().push(record);
    }

    if !args.output_dir.exists() {
        create_dir_all(&args.output_dir).expect("Failed to create output directory");
    }
    map.iter().par_bridge().for_each(|(k, v)| {
        let output_path = args.output_dir.join(format!("{k}.csv"));
        let mut writer = csv::Writer::from_path(&output_path).unwrap();
        println!("{} {}|{} to {}", "Writing".green(), k, v.len(), output_path.display());

        writer.write_record(&header).unwrap();
        for record in v {
            writer.write_record(record).unwrap();
        }
    })
}
