use clap::Parser;
use std::fs;
use std::io;
use std::path::PathBuf;

/// A command-line tool to filter and rename files based on suffixes.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The directory to search for files
    dir: PathBuf,

    /// The original file suffix to look for (e.g., pdf)
    #[arg(short, long, default_value = "pdf")]
    original: String,

    /// The new file suffix to check for (e.g., txt)
    #[arg(short, long, default_value = "txt")]
    examine: String,

    /// The output directory for filtered files (default: ./filtered)
    #[arg(long, default_value = "./filtered")]
    out: PathBuf,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // Ensure the output directory exists
    fs::create_dir_all(&args.out).expect("Failed to create output directory");

    // Collect all files with suffix A in the specified directory that do not have a corresponding file with suffix B
    let files_to_copy: Vec<PathBuf> = fs::read_dir(args.dir)?
        .filter_map(Result::ok) // Filter out any errors reading directory entries
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| path.extension()
            .is_some_and(|ext|
                ext.to_string_lossy()
                    .to_ascii_lowercase()
                    == args.original.to_ascii_lowercase()))
        .filter(|path| !path.with_extension(&args.examine).exists())
        .collect();

    // Copy each collected file to the output directory


    let copied = files_to_copy
        .iter()
        .inspect(
            |file| println!("Copying {}", file.display()),
        )
        .map(|file| {
            let file_name = file.file_name().unwrap_or_default();
            let dest_path = args.out.join(file_name);
            fs::copy(file, &dest_path).expect("Failed to copy file");
        }).count();
    println!("Copied {} files.", copied);

    Ok(())
}




