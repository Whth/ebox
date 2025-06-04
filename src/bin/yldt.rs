use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::path::Path;

#[derive(Parser)]
#[command(name = "yldt")]
#[command(about = "YOLO Dataset Tool - Split datasets and generate data.yaml")]
struct Args {
    /// Image folder path
    #[arg(short, long, default_value = "images")]
    image_dir: String,

    /// Label folder path
    #[arg(short, long, default_value = "labels")]
    label_dir: String,

    /// Output directory
    #[arg(short, long, default_value = "dataset")]
    output_dir: String,

    /// Classes file path
    #[arg(short, long, default_value = "classes.txt")]
    classes_file: String,

    /// Training set ratio (0.0 ~ 1.0)
    #[arg(long, default_value = "0.8")]
    train_ratio: f32,

    /// Don't create validation set
    #[arg(long)]
    no_validation: bool,

    /// Dry run mode (only print without writing)
    #[arg(long)]
    dry_run: bool,

    /// Image file extension
    #[arg(long, default_value = "jpg")]
    image_ext: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Validate parameters
    if args.train_ratio <= 0.0 || args.train_ratio > 1.0 {
        eprintln!("Error: train_ratio must be between 0.0 and 1.0");
        std::process::exit(1);
    }

    println!("🚀 Starting YOLO dataset processing...");

    // Split dataset
    split_dataset(&args)?;

    // Generate data.yaml
    generate_yaml(&args)?;

    println!("✅ Dataset processing completed!");
    Ok(())
}

fn split_dataset(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    // Validate input directories
    validate_input_dirs(args)?;

    // Collect valid file pairs
    let valid_files = collect_valid_file_pairs(args)?;

    // Split dataset
    let (train_files, val_files) = split_files(&valid_files, args.train_ratio, args.no_validation);

    // Create output directory structure
    if !args.dry_run {
        create_output_dirs(&args.output_dir, args.no_validation)?;
    }

    // Copy files
    copy_files(&train_files, args, "train")?;
    if !args.no_validation && !val_files.is_empty() {
        copy_files(&val_files, args, "val")?;
    }

    Ok(())
}

fn validate_input_dirs(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    let image_dir = Path::new(&args.image_dir);
    let label_dir = Path::new(&args.label_dir);

    if !image_dir.exists() {
        return Err(format!("Image directory does not exist: {}", args.image_dir).into());
    }

    if !label_dir.exists() {
        return Err(format!("Label directory does not exist: {}", args.label_dir).into());
    }

    Ok(())
}

fn collect_image_files(
    image_dir: &Path,
    image_ext: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let entries: Result<Vec<_>, _> = fs::read_dir(image_dir)?.collect();
    let entries = entries?;

    let pb = ProgressBar::new(entries.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.set_message("Scanning image files");

    let image_files: Vec<String> = entries
        .par_iter()
        .filter_map(|entry| {
            pb.inc(1);
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext.to_string_lossy().to_lowercase() == image_ext {
                    if let Some(stem) = path.file_stem() {
                        return Some(stem.to_string_lossy().to_string());
                    }
                }
            }
            None
        })
        .collect();

    pb.finish_with_message("Image file scanning completed");

    if image_files.is_empty() {
        return Err(format!("No .{} image files found in directory", image_ext).into());
    }

    Ok(image_files)
}

fn find_valid_pairs(image_files: Vec<String>, label_dir: &Path) -> Vec<String> {
    let pb = ProgressBar::new(image_files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.set_message("Finding valid file pairs");

    let valid_files: Vec<String> = image_files
        .par_iter()
        .filter_map(|image_name| {
            pb.inc(1);
            let label_path = label_dir.join(format!("{}.txt", image_name));
            if label_path.exists() {
                Some(image_name.clone())
            } else {
                eprintln!(
                    "⚠️  Warning: Corresponding label file not found: {}.txt",
                    image_name
                );
                None
            }
        })
        .collect();

    pb.finish_with_message("File pair validation completed");
    valid_files
}

fn collect_valid_file_pairs(args: &Args) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let image_dir = Path::new(&args.image_dir);
    let label_dir = Path::new(&args.label_dir);

    // Collect image files
    let image_files = collect_image_files(image_dir, &args.image_ext)?;

    // Find valid paired files
    let valid_files = find_valid_pairs(image_files, label_dir);

    if valid_files.is_empty() {
        return Err("No paired image and label files found".into());
    }

    println!("📊 Found {} paired files", valid_files.len());
    Ok(valid_files)
}

fn shuffle_files(files: Vec<String>) -> Vec<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut files_with_hash: Vec<(String, u64)> = files
        .into_iter()
        .map(|f| {
            let mut hasher = DefaultHasher::new();
            f.hash(&mut hasher);
            (f, hasher.finish())
        })
        .collect();

    files_with_hash.sort_by_key(|(_, hash)| *hash);
    files_with_hash.into_iter().map(|(f, _)| f).collect()
}

fn split_files(
    valid_files: &[String],
    train_ratio: f32,
    no_validation: bool,
) -> (Vec<String>, Vec<String>) {
    let shuffled_files = shuffle_files(valid_files.to_vec());

    let split_idx = (train_ratio * shuffled_files.len() as f32) as usize;
    let train_files = shuffled_files[..split_idx].to_vec();
    let val_files = if no_validation {
        Vec::new()
    } else {
        shuffled_files[split_idx..].to_vec()
    };

    println!("📈 Training set: {} files", train_files.len());
    if !no_validation {
        println!("📉 Validation set: {} files", val_files.len());
    }

    (train_files, val_files)
}
fn create_output_dirs(
    output_dir: &str,
    no_validation: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let train_img_dir = Path::new(output_dir).join("train").join("images");
    let train_label_dir = Path::new(output_dir).join("train").join("labels");

    fs::create_dir_all(&train_img_dir)?;
    fs::create_dir_all(&train_label_dir)?;

    if !no_validation {
        let val_img_dir = Path::new(output_dir).join("val").join("images");
        let val_label_dir = Path::new(output_dir).join("val").join("labels");

        fs::create_dir_all(&val_img_dir)?;
        fs::create_dir_all(&val_label_dir)?;
    }

    Ok(())
}

fn copy_files(
    files: &[String],
    args: &Args,
    split_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.set_message(format!("Copying {} files", split_name));

    if args.dry_run {
        for file_name in files {
            let src_img =
                Path::new(&args.image_dir).join(format!("{}.{}", file_name, args.image_ext));
            let src_label = Path::new(&args.label_dir).join(format!("{}.txt", file_name));

            let dst_img = Path::new(&args.output_dir)
                .join(split_name)
                .join("images")
                .join(format!("{}.{}", file_name, args.image_ext));
            let dst_label = Path::new(&args.output_dir)
                .join(split_name)
                .join("labels")
                .join(format!("{}.txt", file_name));

            println!("  {} -> {}", src_img.display(), dst_img.display());
            println!("  {} -> {}", src_label.display(), dst_label.display());
            pb.inc(1);
        }
    } else {
        let results: Result<Vec<_>, Box<dyn std::error::Error + Send + Sync>> = files
            .par_iter()
            .map(
                |file_name| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
                    let src_img = Path::new(&args.image_dir)
                        .join(format!("{}.{}", file_name, args.image_ext));
                    let src_label = Path::new(&args.label_dir).join(format!("{}.txt", file_name));

                    let dst_img = Path::new(&args.output_dir)
                        .join(split_name)
                        .join("images")
                        .join(format!("{}.{}", file_name, args.image_ext));
                    let dst_label = Path::new(&args.output_dir)
                        .join(split_name)
                        .join("labels")
                        .join(format!("{}.txt", file_name));

                    fs::copy(&src_img, &dst_img)
                        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
                    fs::copy(&src_label, &dst_label)
                        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;
                    pb.inc(1);
                    Ok(())
                },
            )
            .collect();

        match results {
            Ok(_) => {}
            Err(e) => return Err(format!("File copy error: {}", e).into()),
        }
    }

    pb.finish_with_message(format!("{} set file copying completed", split_name));
    Ok(())
}

fn generate_yaml(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    // Read classes file
    let classes = if Path::new(&args.classes_file).exists() {
        let content = fs::read_to_string(&args.classes_file)?;
        content
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect::<Vec<String>>()
    } else {
        println!(
            "⚠️  Warning: Classes file {} does not exist, will generate empty class list",
            args.classes_file
        );
        Vec::new()
    };

    let train_path = format!("{}/train/images", args.output_dir);
    let val_path = if args.no_validation {
        "".to_string()
    } else {
        format!("{}/val/images", args.output_dir)
    };

    let yaml_content = if args.no_validation {
        format!(
            "train: {}\nval: \nnc: {}\nnames: {:?}",
            train_path,
            classes.len(),
            classes
        )
    } else {
        format!(
            "train: {}\nval: {}\nnc: {}\nnames: {:?}",
            train_path,
            val_path,
            classes.len(),
            classes
        )
    };

    let yaml_path = Path::new(&args.output_dir).join("data.yaml");

    if args.dry_run {
        println!("📄 Will generate data.yaml:");
        println!("{}", yaml_content);
        println!("📍 Path: {}", yaml_path.display());
    } else {
        fs::write(&yaml_path, yaml_content)?;
        println!("📄 Generated data.yaml: {}", yaml_path.display());
    }

    if !classes.is_empty() {
        println!("🏷️  Number of classes: {}", classes.len());
        println!("🏷️  Class names: {:?}", classes);
    }

    Ok(())
}
