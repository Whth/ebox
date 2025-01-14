use clap::{Arg, ArgAction, Command};
use std::{env, fs};
use std::fs::{remove_dir_all};
use std::path::{Path, PathBuf};
use image::{open, GenericImageView, ImageFormat};
use walkdir::WalkDir;
use colored::Colorize;
use rayon::prelude::*;

struct AppArgs {
    input_dir: PathBuf,
    output_dir: PathBuf,
    move_files: bool,
    clean_empty: bool,
    verbose: bool,
    ratios: Vec<(f32, f32)>,
    threads: usize,
}
// Function to parse command line arguments
fn parse_args() -> AppArgs {
    let matches = Command::new("pps")
        .about("Classify images based on aspect ratio and move/copy them.")
        .arg(Arg::new("input-dir")
            .short('i')
            .long("input")
            .value_name("DIR")
            .help("Directory containing nested directories of images")
            .required(true)
        )
        .arg(Arg::new("output-dir")
            .short('o')
            .long("output")
            .value_name("DIR")
            .help("Directory to save the classified images into")
            .default_value("./classified"))
        .arg(Arg::new("move-files")
            .short('m')
            .long("move")
            .help("Move files instead of copying them")
            .action(ArgAction::SetTrue))
        .arg(Arg::new("clean-empty")
            .short('c')
            .long("clean")
            .help("Clean up empty directories after classification")
            .action(ArgAction::SetTrue))
        .arg(Arg::new("verbose")
            .short('v')
            .long("verbose")
            .help("Print detailed information about processed files")
            .action(ArgAction::SetTrue))
        .arg(Arg::new("ratios")
            .short('r')
            .long("ratios")
            .value_name("RATIOS")
            .help("Comma-separated list of aspect ratio ranges in format 'min:max'")
            .default_values(vec!["0:1", "1:8"]))
        .arg(Arg::new("threads")
            .short('t')
            .long("threads")
            .value_name("NUM")
            .help("Number of threads to use for parallel processing")
            .default_value("N"))
        .get_matches();

    let input_dir = matches.get_one::<String>("input-dir").unwrap().as_abs();
    let output_dir = matches.get_one::<String>("output-dir").unwrap().as_abs();
    let move_files = matches.get_flag("move-files");
    let clean_empty = matches.get_flag("clean-empty");
    let verbose = matches.get_flag("verbose");
    let ratios = matches.get_many::<String>("ratios").unwrap_or_default()
        .map(|ratio_str| {
            let parts: Vec<&str> = ratio_str.split(':').collect();
            let min = parts[0].parse::<f32>().unwrap_or(f32::MIN);
            let max = parts[1].parse::<f32>().unwrap_or(f32::MAX);
            (min, max)
        })
        .collect();
    let customized_threads = matches.get_one::<String>("threads").unwrap();
    let threads = if customized_threads != "N" { customized_threads.parse::<usize>().unwrap() } else { num_cpus::get() };
    AppArgs { input_dir, output_dir, move_files, clean_empty, verbose, ratios, threads }
}
// 定义一个新的 trait AsAbsPath
trait AsAbsPath {
    fn as_abs(&self) -> PathBuf;
}

// 实现 AsAbsPath trait 为 String 类型
impl AsAbsPath for String {
    fn as_abs(&self) -> PathBuf {
        let path = Path::new(self);

        if path.is_absolute() {
            // 如果已经是绝对路径，则直接返回
            return path.to_path_buf();
        }

        // 获取当前工作目录
        let current_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        // 将相对路径与当前工作目录合并
        let mut absolute_path = current_dir.join(path);

        // 如果路径是以 `./` 开头，去除前缀
        if let Ok(stripped) = path.strip_prefix("./") {
            absolute_path = current_dir.join(stripped);
        }

        absolute_path
    }
}
// Function to classify an image and move/copy it to the appropriate directory
fn classify_image(path: &Path, input_dir: &Path, output_dir: &Path, move_files: bool, verbose: bool, ratios: &[(f32, f32)]) -> Result<(), Box<dyn std::error::Error>> {
    let img = open(path)?;
    let (width, height) = img.dimensions();
    let aspect_ratio = width as f32 / height as f32;

    let found = ratios.iter().position(|&(min, max)| aspect_ratio >= min && aspect_ratio < max);

    let target_relative_dir = match found {
        Some(index) => {
            let ratio = ratios.get(index).unwrap();
            format!("aspect_{}_{}", ratio.0, ratio.1)
        }
        None => String::from("other"),
    };

    let relative_path_from_input = path.strip_prefix(input_dir).unwrap();
    let target_dir = output_dir.join(&target_relative_dir).join(relative_path_from_input.parent().unwrap_or(input_dir));

    fs::create_dir_all(&target_dir)?;

    let target_path = target_dir.join(relative_path_from_input.file_name().unwrap());

    if verbose {
        println!("Processing file: {} with aspect ratio: {:.2} -> {}",
                 relative_path_from_input.display(), aspect_ratio, target_relative_dir);
    }

    if !move_files {
        fs::copy(path, &target_path)?;
    } else {
        match fs::rename(path, &target_path) {
            Ok(()) => {} // 同一驱动器内移动
            Err(e) if e.raw_os_error() == Some(17) => {
                // 跨驱动器移动
                fs::copy(path, &target_path)?;
                fs::remove_file(path)?;
            }
            Err(e) => println!("Error moving file: {:?}", e), // 其他错误
        }
    }

    Ok(())
}


fn cleanup_empty_directories(root: &Path, verbose: bool) {
    // Collect all directory entries starting from the root.
    WalkDir::new(root)
        .min_depth(1)
        .follow_links(false) // Do not follow symbolic links.
        .into_iter()
        .par_bridge()
        .filter_map(Result::ok)

        .filter_map(|entry| {
            if entry.path().is_file() {
                None
            } else { Some(entry) }
        })

        .filter_map(|entry| {
            if WalkDir::new(entry.path()).into_iter().count() == 1 {
                println!("Found empty directory: {}", entry.path().display().to_string().yellow());
                Some(entry)
            } else {
                println!("Skipping non-empty directory: {}", entry.path().display().to_string().yellow());
                None
            }
        })
        .for_each(|entry|
            {
                let path = entry.path();

                // Ensure the directory is empty before attempting to delete it.
                match remove_dir_all(path) {
                    Ok(_) => {
                        if verbose {
                            println!("Deleted empty directory: {}", path.display().to_string().green());
                        }
                    }
                    Err(e) => println!("Failed to delete empty directory {}: {}", path.display().to_string().red(), e),
                }
            });
}
fn main() {
    let app_args = parse_args();
    // Print initialization information
    println!("{}", "Initializing program...".green());
    println!("Input Directory: {}", app_args.input_dir.display().to_string().magenta());
    println!("Output Directory: {}", app_args.output_dir.display().to_string().bright_magenta());
    println!("Number of Threads: {}", app_args.threads.to_string().blue());
    println!("Move Files: {}", if app_args.move_files { "Yes".red() } else { "No".yellow() });
    println!("Clean Empty Directories: {}", if app_args.clean_empty { "Yes".red() } else { "No".yellow() });
    println!("Verbose Mode: {}", if app_args.verbose { "Enabled" } else { "Disabled" });
    println!("Aspect Ratio Ranges:");
    for (index, ratio) in app_args.ratios.iter().enumerate() {
        println!("  Range {}: {:.2}:{:.2}", index, ratio.0, ratio.1);
    }

    // 确保输出目录存在
    if app_args.output_dir.exists() {
        println!("{}", format!("{} already exists!", app_args.output_dir.display()).red());
        return;
    };
    fs::create_dir_all(&app_args.output_dir).expect("Failed to create output directory");

    // 获取默认的线程数量或者用户指定的线程数量
    rayon::ThreadPoolBuilder::new().num_threads(app_args.threads).build_global().unwrap();

    // 并行遍历目录中的每个文件
    WalkDir::new(&app_args.input_dir)
        .into_iter()
        .par_bridge()
        .filter_map(Result::ok)
        .for_each(|entry| {
            let path = entry.path();
            if path.is_file() && ImageFormat::from_path(path).is_ok() {
                // 注意这里不能直接返回Result，因为Rayon无法处理异步错误
                classify_image(path, &app_args.input_dir, &app_args.output_dir, app_args.move_files, app_args.verbose, &app_args.ratios)
                    .unwrap_or_else(|e| eprintln!("Failed to process file: {}", e));
            }
        });

    if app_args.clean_empty {
        println!("Starting cleanup...");
        cleanup_empty_directories(&app_args.input_dir, app_args.verbose);
    }
}