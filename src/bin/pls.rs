use clap::{Parser, Subcommand};
use image::GenericImageView;
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;

/// A CLI tool for identifying, classifying, extracting, and managing images based on their properties.
#[derive(Parser)]
#[clap(author, version, about)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Identify the type of an image: grayscale, colorful, or with transparency.
    Identify {
        /// List of image paths to be checked.
        #[clap(required = true)]
        images: Vec<PathBuf>,
    },
    /// Classify the type of each image in a directory: grayscale, colorful, or with transparency.
    Classify {
        /// Root directory containing images to classify.
        #[clap(short, long, required = true)]
        root_dir: PathBuf,
    },
    /// Check the difference between color channels of an image to determine its grayscale level.
    CheckDiff {
        /// The image file to check.
        #[clap(required = true)]
        image: PathBuf,
        /// The threshold value for determining if an image is grayscale.
        #[clap(short, long, default_value_t = 0.02)]
        threshold: f64,
    },

    /// Extract images from a directory based on their type: grayscale, colorful, or with transparency.
    Extract {
        /// Filter type: gsc (grayscale), col (colorful), tra (transparent), ntra (not transparent).
        #[arg(short, long, default_value_t = String::from("gsc"))]
        filter_type: String,
        /// Input directory containing images to extract.
        #[clap(short, long, required = true)]
        input_dir: PathBuf,
        /// Output directory where extracted images will be moved.
        #[clap(short, long)]
        output_dir: Option<PathBuf>,
        /// The threshold value for determining if an image is grayscale.
        #[clap(short, long, default_value_t = 0.02)]
        threshold: f64,
    },

    /// Move images smaller than the specified size limit to the output directory.
    Small {
        /// Input directory containing images to process.
        #[clap(short, long, required = true)]
        input_dir: PathBuf,
        /// Output directory where small images will be moved.
        #[clap(short, long)]
        output_dir: Option<PathBuf>,
        /// Size limit of the image file, unit: Mb.
        #[clap(short, long, default_value_t = 0.2)]
        size: f64,
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Identify { images } => identify_images(images),
        Commands::Classify { root_dir } => classify_images(root_dir),
        Commands::CheckDiff { image, threshold } => check_diff(image, *threshold),
        Commands::Extract { filter_type, input_dir, output_dir, threshold } => extract_images(filter_type, input_dir, output_dir.clone(), *threshold),
        Commands::Small { input_dir, output_dir, size } => small_images(input_dir, output_dir.clone(), *size),
    }
}

fn identify_images(images: &[PathBuf]) {
    for img_path in images {
        if let Ok((is_grayscale, has_alpha)) = get_image_properties(img_path) {
            if is_grayscale {
                println!("{} is Grayscale.", img_path.display());
            } else {
                println!("{} is Colorful.", img_path.display());
            }
            if has_alpha {
                println!("{} has Transparency.", img_path.display());
            } else {
                println!("{} is neither transparent nor grayscale.", img_path.display());
            }
        } else {
            println!("Error processing {}.", img_path.display());
        }
    }
}

fn classify_images(root_dir: &PathBuf) {
    // Placeholder implementation for classification
    println!("Classifying images in {:?}", root_dir);
}

fn check_diff(image: &PathBuf, threshold: f64) {
    if let Ok(_) = get_image_properties(image) {
        let gray_diff = calculate_gray_difference(image).unwrap_or(0.0);
        let total_pixels = image::open(image).map(|img| img.width() * img.height()).unwrap_or(0);
        let grayscale_threshold = total_pixels as f64 * threshold;
        println!("The diff is {}", gray_diff - grayscale_threshold);
    } else {
        println!("Error processing {}.", image.display());
    }
}

fn extract_images(filter_type: &str, input_dir: &PathBuf, output_dir: Option<PathBuf>, threshold: f64) {
    let output_dir = output_dir.unwrap_or_else(|| input_dir.join(format!("-{}", filter_type)));
    fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    let filter_func: Box<dyn Fn(&PathBuf) -> bool> = match filter_type {
        "gsc" => Box::new(move |p: &PathBuf| is_grayscale(p, threshold)),
        "col" => Box::new(move |p: &PathBuf| !is_grayscale(p, threshold)),
        "tra" => Box::new(is_transparent),
        "ntra" => Box::new(move |p: &PathBuf| !is_transparent(p)),
        _ => panic!("Invalid filter type"),
    };

    find_files_by_extensions_recursively(input_dir, &[".jpg", ".jpeg", ".png"])
        .iter()
        .filter(|img_path| filter_func(img_path))
        .for_each(|img_path| {
            move_file_with_conflict_handling(&img_path, &output_dir);
            println!("Extracted {:?}", img_path.strip_prefix(input_dir).unwrap());
        });
}

fn small_images(input_dir: &PathBuf, output_dir: Option<PathBuf>, size_limit_mb: f64) {
    let output_dir = output_dir.unwrap_or_else(|| input_dir.join("-small"));
    fs::create_dir_all(&output_dir).expect("Failed to create output directory");
    let size_limit_bytes = (size_limit_mb * 1024.0 * 1024.0) as u64;

    find_files_by_extensions_recursively(input_dir, &[".jpg", ".jpeg", ".png"])
        .into_par_iter()
        .for_each(|img_path| {
            if img_path.metadata().map(|m| m.len() < size_limit_bytes).unwrap_or(false) {
                move_file_with_conflict_handling(&img_path, &output_dir);
                println!("Moved {:?}", img_path.strip_prefix(input_dir).unwrap());
            }
        });
}

fn get_image_properties(path: &PathBuf) -> Result<(bool, bool), String> {
    let img = image::open(path).map_err(|_| format!("Error reading image: {:?}", path))?;
    let is_grayscale = img.color() == image::ColorType::L8 || img.color() == image::ColorType::La8;
    let has_alpha = img.color().has_alpha();
    Ok((is_grayscale, has_alpha))
}

fn is_grayscale(path: &PathBuf, threshold: f64) -> bool {
    let img = image::open(path).expect("Error reading image");
    let gray_diff = calculate_gray_difference(path).unwrap_or(0.0);
    let total_pixels = img.width() * img.height();
    gray_diff < total_pixels as f64 * threshold
}

fn calculate_gray_difference(path: &PathBuf) -> Result<f64, String> {
    let img = image::open(path).map_err(|_| format!("Error reading image: {:?}", path))?;
    let pixels: Vec<_> = img.pixels().collect();
    let mut bg_diff = 0.0;
    let mut gr_diff = 0.0;
    let mut rb_diff = 0.0;

    for i in 0..pixels.len() {
        let [r, g, b, _] = pixels[i].2.0;
        bg_diff += (r as f64 - g as f64).abs();
        gr_diff += (g as f64 - b as f64).abs();
        rb_diff += (r as f64 - b as f64).abs();
    }

    Ok(bg_diff + gr_diff + rb_diff)
}

fn is_transparent(path: &PathBuf) -> bool {
    let img = image::open(path).expect("Error reading image");
    img.color().has_alpha()
}

fn find_files_by_extensions_recursively(root_path: &PathBuf, extensions: &[&str]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(root_path) {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() && extensions.iter().any(|ext| path.extension().and_then(|e| e.to_str()) == Some(ext)) {
                files.push(path.to_path_buf());
            }
        }
    }
    files
}

fn move_file_with_conflict_handling(src_path: &PathBuf, dst_dir: &PathBuf) {
    let base_name = src_path.file_name().unwrap();
    let mut dst_path = dst_dir.join(base_name);
    let mut counter = 1;

    while dst_path.exists() {
        let name = src_path.file_stem().unwrap().to_string_lossy();
        let ext = src_path.extension().unwrap().to_string_lossy();
        let new_base_name = format!("{}_{}.{}", name, counter, ext);
        dst_path = dst_dir.join(new_base_name);
        counter += 1;
    }

    fs::rename(src_path, &dst_path).expect("Failed to move file");
}



