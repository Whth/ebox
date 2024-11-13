use clap::Parser;
use chrono::Duration;
use ffmpeg_next as ffmpeg;
use std::fs::{File};
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command;
use walkdir::WalkDir;



#[derive(Parser)]
#[command(name = "video_concatenator")]
#[command(author, version, about, long_about = "Concatenate videos with ffmpeg")]
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



}




fn main() -> io::Result<()> {
    let cli = Cli::parse();
    println!("Using directory: {}", cli.dir);


    let dir_path = PathBuf::from(&cli.dir);
    let max_duration = Duration::seconds(cli.max_duration as i64);

    // 列出目录中的所有视频文件
    let video_files: Vec<PathBuf> = WalkDir::new(&dir_path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "mp4" || ext == "avi" || ext == "mkv"))
        .map(|e| e.path().to_path_buf())
        .collect();

    // 过滤出长度小于指定长度的视频文件
    let filtered_videos: Vec<PathBuf> = video_files
        .into_iter()
        .filter(|path| {
            let duration = ffmpeg::format::input(&path.to_str().unwrap()).unwrap().duration();
            let duration_seconds = duration;
            duration_seconds < max_duration.num_seconds()
        })
        .collect();

    if filtered_videos.is_empty() {
        eprintln!("No videos found with duration less than {} seconds.", cli.max_duration);
        return Ok(());
    }

    // 生成包含视频文件路径的文本文件
    let list_file_path = "video_list.txt";
    let mut list_file = File::create(list_file_path).expect("Failed to create list file");
    for path in &filtered_videos {
        writeln!(list_file, "file '{}'", path.display()).expect("Failed to write to list file");
    }

    // 使用 ffmpeg 拼接视频文件
    let output_file = &cli.output;

    let mut command = Command::new("ffmpeg");

    command
        .args(&["-f", "concat"])
        .arg("-safe")
        .arg("0")
        .arg("-i")
        .arg(list_file_path)
        .arg("-c")
        .arg("copy")
        .arg(output_file);

    if cli.use_nvenc {
        command.arg("-c:v").arg("h264_nvenc").arg("-c:a").arg("copy");
    }

    let output = command.output().expect("Failed to execute FFmpeg command");

    if output.status.success() {
        println!("FFmpeg command executed successfully.");
    } else {
        eprintln!("FFmpeg command failed with status: {}", output.status);
        eprintln!("Stdout: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
    }
    if !output.status.success() {
        eprintln!("Failed to concatenate videos: {:?}", output.stderr);
        return Err(io::Error::new(io::ErrorKind::Other, "Failed to concatenate videos"));
    }

    println!("Videos concatenated successfully to {}", output_file);
    Ok(())
}