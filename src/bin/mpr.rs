use clap::Parser;
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A Cli that can be used to process a directory of markdown files with images.
#[derive(Parser)]

struct Cli {
    /// Parent directory containing target directories
    parent_dir: PathBuf,

    /// Starting number for image renaming
    #[clap(short, long, default_value = "1")]
    start: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    let parent_dir = args.parent_dir.canonicalize()?;
    let start_number = args.start;

    if !parent_dir.is_dir() {
        eprintln!("Error: {:?} is not a directory", parent_dir);
        std::process::exit(1);
    }

    fs::read_dir(&parent_dir)
        .expect("Failed to read directory")
        .par_bridge()
        .filter_map(|entry| entry.ok())
        .for_each(|entry| {
            let path = entry.path();

            if path.is_dir() {
                match process_target_directory(&path, start_number) {
                    Ok(_) => println!("✅ Processed {}: {:?}", start_number, path),
                    Err(e) => eprintln!("❌ Failed {}: {:?}: {}", start_number, path, e),
                }
            }
        });

    Ok(())
}

/// Process a single target directory with specified starting number
fn process_target_directory(dir: &Path, start: u32) -> Result<(), Box<dyn std::error::Error>> {
    let (md_file, images_dir) = validate_directory(dir)?;
    let md_content = fs::read_to_string(&md_file)?;

    let image_references = process_markdown_images(&md_content)?;
    let path_map = rename_images(&image_references, dir, &images_dir, start)?;

    update_markdown_content(&md_file, &md_content, &path_map)
}

/// Validates directory structure and returns required paths
fn validate_directory(dir: &Path) -> Result<(PathBuf, PathBuf), &'static str> {
    if !dir.is_dir() {
        return Err("Invalid directory");
    }

    let md_files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|_| "Failed to read directory")?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().is_file()
                && entry.path().extension().and_then(|s| s.to_str()) == Some("md")
        })
        .map(|entry| entry.path())
        .collect();

    if md_files.is_empty() {
        return Err("No .md file found");
    }
    if md_files.len() > 1 {
        return Err("Multiple .md files found");
    }

    let images_dir = dir.join("images");
    if !images_dir.is_dir() {
        return Err("images directory not found");
    }

    Ok((md_files[0].clone(), images_dir))
}

/// Extracts image paths from markdown content
fn process_markdown_images(md_content: &str) -> Result<Vec<String>, regex::Error> {
    let re = Regex::new(r"!\[.*?]\((.*?)\)")?;
    Ok(re
        .captures_iter(md_content)
        .filter_map(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
        .collect())
}

/// Renames images with configurable starting number
fn rename_images(
    image_paths: &[String],
    base_dir: &Path,
    images_dir: &Path,
    start: u32,
) -> Result<HashMap<String, String>, std::io::Error> {
    let mut path_map = HashMap::new();

    for original_path in image_paths {
        if !original_path.starts_with("images/") {
            continue;
        }

        let original_full_path = base_dir.join(original_path);
        if !original_full_path.exists() {
            eprintln!("  ⚠️ Missing: {:?}", original_full_path);
            continue;
        }

        let extension = original_full_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let mut new_number = start + path_map.len() as u32;
        let mut new_filename = format!("{}.{}", new_number, extension);
        let mut new_full_path = images_dir.join(&new_filename);

        while new_full_path.exists() {
            new_number += 1;
            new_filename = format!("{}.{}", new_number, extension);
            new_full_path = images_dir.join(&new_filename);
        }

        fs::rename(&original_full_path, &new_full_path)?;
        path_map.insert(original_path.clone(), format!("images/{}", new_filename));
    }

    Ok(path_map)
}

/// Updates markdown content with new image paths
fn update_markdown_content(
    md_file: &Path,
    md_content: &str,
    path_map: &HashMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let re = Regex::new(r"!\[(?P<alt>.*?)]\((?P<path>.*?)\)")?;
    let new_content = re.replace_all(md_content, |caps: &regex::Captures| {
        let alt = &caps["alt"];
        let path = &caps["path"];

        match path_map.get(path) {
            Some(new_path) => format!("![{}]({})", alt, new_path),
            None => caps.get(0).unwrap().as_str().to_string(),
        }
    });

    fs::write(md_file, new_content.as_bytes())?;
    Ok(())
}
