use clap::Parser;
use std::fs::{self};
use std::path::PathBuf;

/// A command-line tool for reducing the depth of a directory hierarchy.
#[derive(Parser)]
#[command(version)]
#[command(author)]
struct Cli {
    /// Specifies the input directory where files and directories will be processed.
    /// - Command-line Flags: `-i`, `--input-dir`
    /// - Default Value: Current working directory (`./`)
    /// - Value Parser: Uses the default value parser provided by `clap`
    #[arg(short, long, default_value = "./")]
    input_dir: PathBuf,

    /// Specifies the output directory where processed files and directories will be moved.
    /// - Command-line Flags: `-o`, `--output-dir`
    /// - Default Value: Current working directory (`./`)
    /// - Value Parser: Uses the default value parser provided by `clap`
    #[arg(short, long, default_value = "./")]
    output_dir: PathBuf,

    /// Defines the depth level to which the program should traverse subdirectories.
    /// A depth of `1` means only the immediate contents of the input directory are processed.
    /// - Command-line Flags: `-d`, `--depth`
    /// - Default Value: `1`
    #[arg(short, long, default_value_t = 1)]
    depth: u32,

    /// Determines how the program handles file or directory collisions during the move operation.
    /// Available strategies are:
    /// - `auto`: Automatically renames the destination file or directory if a collision occurs.
    /// - `override`: Overwrites the existing file or directory at the destination.
    /// - `halt`: Stops execution if a collision is detected.
    /// - Command-line Flags: `-c`, `--collision-strategy`
    /// - Default Value: `"auto"`
    #[arg(short, long, default_value = "auto")]
    collision_strategy: String,

    /// Enables verbose mode, providing detailed output about the operations being performed.
    /// - Command-line Flags: `-v`, `--verbose`
    /// - Action: Flagged presence sets this to `true`; absence sets it to `false`.
    #[arg(short, long, action)]
    verbose: bool,
}

fn rename_if_exists(path: &PathBuf) -> PathBuf {
    let mut i = 1;
    let mut new_path = path.clone();
    while new_path.exists() {
        new_path.set_file_name(format!("{}_{}", path.file_stem().unwrap().to_string_lossy(), i));
        new_path.set_extension(path.extension().unwrap_or_default());
        i += 1;
    }
    new_path
}
fn handle_collision(src_path: &PathBuf, dest_path: &PathBuf, collision_strategy: &str, verbose: bool) -> Option<PathBuf> {
    match collision_strategy.to_lowercase().as_str() {
        "auto" => auto_strategy(src_path, dest_path, verbose),
        "override" => override_strategy(dest_path, verbose),
        "halt" => halt_strategy(dest_path),
        _ => unreachable!("Unknown collision strategy"),
    }
}

fn auto_strategy(src_path: &PathBuf, dest_path: &PathBuf, verbose: bool) -> Option<PathBuf> {
    if dest_path.exists() && src_path.canonicalize().ok() == dest_path.canonicalize().ok() {
        if verbose {
            println!("Skipping {} as it matches {}.", src_path.display(), dest_path.display());
        }
        return None;
    }

    if dest_path.exists() {
        let new_dest_path = rename_if_exists(dest_path);
        if verbose {
            println!("Renaming {} to {} to avoid collision.", dest_path.display(), new_dest_path.display());
        }
        Some(new_dest_path)
    } else {
        Some(dest_path.to_path_buf())
    }
}
fn halt_strategy(dest_path: &PathBuf) -> Option<PathBuf> {
    if dest_path.exists() {
        panic!("Destination path {} already exists.", dest_path.display());
    }
    Some(dest_path.to_path_buf())
}
fn override_strategy(dest_path: &PathBuf, verbose: bool) -> Option<PathBuf> {
    if dest_path.exists() {
        if dest_path.is_dir() {
            fs::remove_dir_all(dest_path).expect("Failed to remove directory");
        } else {
            fs::remove_file(dest_path).expect("Failed to remove file");
        }
        if verbose {
            println!("Overriding {} with a new file or directory.", dest_path.display());
        }
    }
    Some(dest_path.to_path_buf())
}
fn resolve_move(src_path: &PathBuf, dest_path: &PathBuf, verbose: bool) {
    fs::rename(src_path, dest_path).expect("Failed to move file/directory");
    if verbose {
        println!("Moved {} to {}.", src_path.display(), dest_path.display());
    }
}

fn move_dir_content(src_dir: &PathBuf, dest_dir_path: &PathBuf, verbose: bool, strategy: &str) {
    fs::read_dir(src_dir)
        .expect("Failed to read directory")
        .filter_map(Result::ok)
        .for_each(|entry| {
            let dest_f = dest_dir_path.join(entry.file_name());
            if let Some(handled_dest) = handle_collision(&entry.path(), &dest_f, strategy, verbose) {
                resolve_move(&entry.path(), &handled_dest, verbose);
            }
        });
}

fn is_directory_empty(directory: &PathBuf) -> bool {
    fs::read_dir(directory).map(|mut entries| entries.next().is_none()).unwrap_or(false)
}

fn expand_directories(input_dir: &PathBuf, output_dir: &PathBuf, depth: u32, collision_strategy: &str, verbose: bool) {
    let initial_depth = depth;
    let mut queue = vec![(input_dir.clone(), output_dir.clone(), initial_depth)];

    while let Some((current_input_dir, current_output_dir, current_depth)) = queue.pop() {
        if current_depth == 0 {
            move_dir_content(&current_input_dir, &current_output_dir, verbose, collision_strategy);
        } else {
            let dirs_to_move: Vec<_> = fs::read_dir(&current_input_dir)
                .expect("Failed to read directory")
                .filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .collect();

            let remaining_depth = current_depth - 1;
            for dir in dirs_to_move {
                queue.push((dir.path(), current_output_dir.clone(), remaining_depth));
            }
        }
    }

    fs::read_dir(input_dir)
        .expect("Failed to read directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir() && is_directory_empty(&entry.path()))
        .for_each(|entry| {
            fs::remove_dir_all(&entry.path()).expect("Failed to clean directory");
            if verbose {
                println!("Cleaning {}", entry.path().display());
            }
        });
}

fn main() {
    let cli = Cli::parse();

    if cli.depth < 1 {
        eprintln!("Depth can't be smaller than 1!");
        return;
    }

    expand_directories(&cli.input_dir, &cli.output_dir, cli.depth, &cli.collision_strategy, cli.verbose);
}



