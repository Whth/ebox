use clap::Parser;
use log::{error, info};
use rayon::prelude::*;
use std::fs::create_dir_all;
use std::path::PathBuf;
use std::process::Command;
use walkdir::WalkDir;
/// Command-line arguments for the audio resampling tool
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input directory containing audio files
    #[arg(short, long)]
    input: PathBuf,

    /// Output directory to write the resampled audio files
    #[arg(short, long, default_value = "./resampled")]
    output: PathBuf,

    /// Audio bitrate in kbps
    #[arg(short, long, default_value_t = 320)]
    bitrate: u32,

    /// Audio sample rate in Hz
    #[arg(short, long, default_value_t = 48000)]
    sample_rate: u32,

    /// Verbosity level
    #[arg(short, long, action = clap::ArgAction::Count, default_value_t = 0)]
    verbose: u8,

    /// Target file extension
    #[arg(short, long, default_value = "mp3")]
    target_extension: String,
}


fn main() {
    env_logger::init();

    let args = Args::parse();

    let input_dir = args.input;
    let output_dir = args.output;
    let bitrate = args.bitrate;
    let sample_rate = args.sample_rate;
    let verbose = args.verbose > 0;
    let target_extension = args.target_extension;

    if verbose {
        info!("Verbose mode enabled.");
    }

    info!("Input Directory: {:?}", input_dir);
    info!("Output Directory: {:?}", output_dir);
    info!("Bitrate: {} kbps", bitrate);
    info!("Sample Rate: {} Hz", sample_rate);

    // Create the output directory if it doesn't exist
    create_dir_all(&output_dir).expect("Failed to create output directory.");

    // Walk through the input directory and process each file
    WalkDir::new(&input_dir)
        .into_iter()
        .par_bridge()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            let path = entry.path().to_owned();
            let relative_path = path.strip_prefix(&input_dir).unwrap();
            let mut output_path = output_dir.join(relative_path);
            output_path.set_extension(target_extension.as_str());
            create_dir_all(output_path.parent().unwrap()).expect("Failed to create directory structure.");

            (Command::new("ffmpeg").arg("-i")
                 .arg(&path)
                 .arg("-vn")
                 .arg("-b:a")
                 .arg(format!("{}k", bitrate))
                 .arg("-ar")
                 .arg(sample_rate.to_string())
                 .arg(&output_path)
                 .arg("-y")
                 .stdout(std::process::Stdio::null())
                 .status()
                 .expect("Failed to execute ffmpeg command.")
                 .success(), path, output_path)
        })
        .for_each(|(success, path, output_path)| if success {
            info!("Resampled|OUT {}", output_path.display());
        } else {
            error!("Failed to resample {}", path.display());
        });
}

