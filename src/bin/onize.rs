use clap::Parser;
use regex::Regex;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

/// Concatenate .txt files in a directory
#[derive(Parser)]
#[command(author, version, long_about = None
)]
struct Cli {
    /// The output file path
    #[arg(value_name = "FILE", default_value = "output.txt")]
    output: PathBuf,
    /// The directory containing the .txt files
    #[arg(value_name = "DIR", default_value = "./")]
    directory: PathBuf,

}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    if !cli.directory.is_dir() {
        eprintln!("Error: The specified directory does not exist.");
        return Err(io::Error::from(io::ErrorKind::NotFound));
    }

    let re = Regex::new(r"(\d+).*").unwrap(); // 正则表达式用于匹配文件名中的数字

    let mut entries: Vec<_> = fs::read_dir(cli.directory)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file() && entry.file_name().to_string_lossy().ends_with(".txt"))
        .filter(|entry| entry.path() != cli.output)
        .collect();

    // 使用正则表达式提取文件名中的数字并排序
    entries.sort_by_cached_key(|entry| {
        let file_name = entry.file_name().clone();
        // 查找所有匹配项，并尝试将第一个匹配项转换为整数
        re.captures(&file_name.to_string_lossy())
            .and_then(|caps| caps.get(1).map(|m| m.as_str()))
            .and_then(|num_str| num_str.parse::<i32>().ok())
            .unwrap_or(0) // 如果没有匹配到数字，则默认值为0
    });

    let mut output_file = fs::File::create(&cli.output)?;

    for entry in entries {
        let content = fs::read_to_string(entry.path())?;
        writeln!(output_file, "{}", content)?;
    }

    println!("Files have been concatenated and written to {}", cli.output.display());

    Ok(())
}



