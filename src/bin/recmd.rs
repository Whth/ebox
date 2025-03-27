use anyhow::Result;
use clap::Parser;
use rayon::prelude::*;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// CLI tool to execute commands in all subdirectories of the current directory
#[derive(Parser, Debug)]
#[command(name = "dir-runner")]
#[command(about = "Execute a command in all subdirectories of the current directory")]
struct Args {
    #[arg(short, long, help = "Recursively traverse subdirectories")]
    recursive: bool,

    #[arg(
        short,
        long,
        help = "Exclude directories matching the specified pattern"
    )]
    exclude: Option<String>,

    #[arg(short, long, help = "Display the command without executing it")]
    dry_run: bool,

    #[arg(required = true, help = "The command to execute")]
    command: Vec<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let current_dir = env::current_dir()?;
    let mut directories = Vec::new();

    // Collect directories that meet the criteria
    collect_directories(
        &current_dir,
        &mut directories,
        args.recursive,
        &args.exclude,
    )?;

    // Execute the command in each directory in parallel
    directories.par_iter().for_each(|dir| {
        if let Err(e) = execute_command_in_dir(dir, &args.command, args.dry_run) {
            eprintln!("Error in directory {}: {}", dir.display(), e);
        }
    });

    Ok(())
}

/// Recursively collect directories
fn collect_directories(
    path: &Path,
    dirs: &mut Vec<PathBuf>,
    recursive: bool,
    exclude: &Option<String>,
) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Exclude directory check
            if let Some(pattern) = exclude {
                if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map_or(false, |name| name.contains(pattern))
                {
                    continue;
                }
            }

            dirs.push(path.clone());

            // Recurse into subdirectories
            if recursive {
                collect_directories(&path, dirs, recursive, exclude)?;
            }
        }
    }

    Ok(())
}

/// Execute the command in the specified directory
fn execute_command_in_dir(dir: &Path, command: &[String], dry_run: bool) -> Result<()> {
    println!("Entering directory: {}", dir.display());

    if !dry_run {
        let mut cmd = Command::new(&command[0]);
        cmd.args(&command[1..]);
        cmd.current_dir(dir);

        let status = cmd.status()?;

        if !status.success() {
            eprintln!(
                "Command failed in {}, exit code: {}",
                dir.display(),
                status.code().unwrap_or(-1)
            );
        }
    } else {
        println!(
            "(Dry run) Will execute in {}: {}",
            dir.display(),
            command.join(" ")
        );
    }

    Ok(())
}
