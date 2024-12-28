use clap::{Arg, ArgAction, Command};
use rayon::prelude::*;
use std::fs;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
fn main() -> io::Result<()> {
    let matches = Command::new("wprune")
        .version("1.0")
        .author("Your Name")
        .about("Removes specified patterns from all Markdown files in a directory")
        .arg(
            Arg::new("patterns")
                .short('r')
                .long("remove-r")
                .action(ArgAction::SetTrue)
                .help("Remove ###"),
        )
        .arg(
            Arg::new("stars")
                .short('s')
                .long("remove-stars")
                .action(ArgAction::SetTrue)
                .help("Remove **"),
        )
        .arg(
            Arg::new("hyphens")
                .short('k')
                .long("remove-hyphens")
                .action(ArgAction::SetTrue)
                .help("Remove -"),
        )
        .arg(
            Arg::new("INPUT_DIR")
                .index(1)
                .required(true)
                .help("Path to the input directory containing Markdown files"),
        )
        .arg(
            Arg::new("OUTPUT_DIR")
                .index(2)
                .required(true)
                .help("Path to the output directory where cleaned Markdown files will be saved"),
        )
        .arg(
            Arg::new("EXT")
                .index(3)
                .required(true)
                .help("Perform a dry run without modifying the files"),
        )
        .get_matches();

    // Get the paths for the input and output directories
    let input_dir = matches.get_one::<String>("INPUT_DIR").unwrap();
    let output_dir = matches.get_one::<String>("OUTPUT_DIR").unwrap();
    let ext = matches.get_one::<String>("EXT").unwrap();

    // Ensure the output directory exists
    fs::create_dir_all(output_dir)?;

    // Determine which patterns to remove
    let remove_patterns = *matches.get_one::<bool>("patterns").unwrap_or(&false);
    let remove_stars = *matches.get_one::<bool>("stars").unwrap_or(&false);
    let remove_hyphens = *matches.get_one::<bool>("hyphens").unwrap_or(&false);

    // Read the entries in the input directory and process each file
    let count = fs::read_dir(input_dir)
        .expect("Failed to read input directory")
        .par_bridge()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file() && entry.path().extension().and_then(|ext| ext.to_str()) == Some(ext))
        .inspect(|entry| println!("Processing {}", entry.path().display()))
        .map(|entry| {
            let path = entry.path();

            // Read the content of the file
            let content = fs::read_to_string(&path).expect("Failed to read file");

            // Remove specified patterns
            let mut cleaned_content = content;
            if remove_patterns {
                cleaned_content = cleaned_content
                    .replace("### ", "")
                    .replace("## ", "")
                    .replace("# ", "");
            }
            if remove_stars {
                cleaned_content = cleaned_content.replace("**", "");
            }
            if remove_hyphens {
                cleaned_content = cleaned_content.replace("- ", "");
            }

            // Create the corresponding output file path
            let output_path = Path::new(output_dir).join(path.file_name().unwrap());

            // Write the cleaned content to the output file
            let mut file = File::create(&output_path).expect("Failed to create output file");
            file.write_all(cleaned_content.as_bytes()).expect("Failed to write to output file");
        }).count();

    println!("{} files processed", count);
    Ok(())
}



