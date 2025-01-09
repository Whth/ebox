use clap::Parser;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Convert specified directories into files
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Directory paths to be converted
    #[arg(value_name = "DIR_PATHS")]
    dir_paths: Vec<String>,

    /// Replace all directories under the current directory with files
    #[arg(short, long)]
    all: bool,
}

fn main() {
    let cli = Cli::parse();

    // Get the list of paths to process
    let paths_to_process = if cli.all {
        fs::read_dir(env::current_dir().unwrap())
            .unwrap_or_else(|_| {
                eprintln!("Cannot read the current directory");
                std::process::exit(1);
            })
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .map(|entry| entry.path())
            .collect::<Vec<_>>()
    } else {
        cli.dir_paths.iter()
            .map(Path::new)
            .map(|path| path.to_path_buf())
            .collect::<Vec<PathBuf>>()
    };

    // Iterate over each path and try to convert it into a file
    paths_to_process.iter()
        .for_each(|path| {
            if path.is_dir() {
                // Remove the directory and its contents, then create a same-named file in its place
                match fs::remove_dir_all(path) {
                    Ok(_) => match fs::File::create(path.with_extension("")) {
                        Ok(_) => println!("Successfully converted {} into a file", path.display()),
                        Err(e) => eprintln!("Failed to create file {}: {}", path.display(), e),
                    },
                    Err(e) => eprintln!("Failed to delete directory {}: {}", path.display(), e),
                }
            } else {
                eprintln!("{} is not a valid directory", path.display());
            }
        });
}