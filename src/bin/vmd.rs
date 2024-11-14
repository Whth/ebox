use clap::Parser;
use chrono::Duration;
use ffmpeg_next as ffmpeg;
use std::fs::{File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use walkdir::WalkDir;
use rayon::prelude::*;


#[derive(Parser)]
#[command(name = "video_concatenator")]
#[command(author, version, about, long_about = "Concatenate videos with ffmpeg")]
/// Concatenate videos under a given directory with a duration smaller than the given one using ffmpeg
struct Cli {
    /// the input directory
    #[arg(short, long, default_value = "./")]
    dir: String,


    /// the maximum duration of the input video
    #[arg(short, long, default_value = "15")]
    max_duration: u64,




    /// the output file
    #[arg(short, long, default_value = "output.mp4")]
    output: String,


    /// use nvenc
    #[arg(long,short)]
    use_nvenc: bool,

    /// list file path
    #[arg(long,short,default_value = "list.txt")]
    list_file_path : String,


}




fn main() -> io::Result<()> {
    let cli = Cli::parse();
    println!("Using directory: {}", cli.dir);
    println!("Merge videos with duration less than {} seconds.",cli.max_duration);

    let dir_path = PathBuf::from(&cli.dir);
    let max_duration = Duration::seconds(cli.max_duration as i64);

    // 列出目录中的所有视频文件
    let video_files: Vec<PathBuf> = WalkDir::new(&dir_path)
        .into_iter()
        .par_bridge()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "mp4" || ext == "avi" || ext == "mkv"))
        .map(|e| e.path().to_path_buf())
        .collect();

    println!("Found {} videos:", video_files.len());
    // 过滤出长度小于指定长度的视频文件
    let filtered_videos: Vec<PathBuf> = video_files
        .into_iter()
        .par_bridge()
        .filter(|path| {

            let duration = ffmpeg::format::input(path)
                .expect("Failed to extract metadata from video.")
                .streams()
                .map(|s|(s.duration() as f64*s.time_base().numerator() as f64/s.time_base().denominator() as f64) as i64)
                .max()
                .expect("No streams found in video.");
            println!("{} => duration: {}", path.display(), duration);
            duration < max_duration.num_seconds()
        })
        .collect();

    println!("Filtered {} videos:", filtered_videos.len());
    if filtered_videos.is_empty() {
        eprintln!("No videos found with duration less than {} seconds.", cli.max_duration);
        return Ok(());
    }

    // 生成包含视频文件路径的文本文件
    let mut list_file = File::create(&cli.list_file_path).expect("Failed to create list file");
    filtered_videos
        .iter()
        .for_each(|path| {
            write!(list_file, "file '{}'\n", path.display()).expect("Failed to write to list file");
        });
    println!("saved list file to {}",&cli.list_file_path);

    // 使用 ffmpeg 拼接视频文件
    let output_file = &cli.output;

    let mut command = Command::new("ffmpeg");

    command
        .args(&["-f", "concat"])
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(&cli.list_file_path)
        .arg("-c")
        .arg("copy")
        .arg(output_file);

    if cli.use_nvenc {
        command.arg("-c:v").arg("h264_nvenc").arg("-c:a").arg("copy");
    }

    let output = command.status().expect("Failed to execute FFmpeg command").success();

    if output {
        println!("Videos concatenated successfully to {}", output_file);
    } else {
        eprintln!("Failed to concatenate videos!");
    }

    Ok(())
}