use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::{copy, create_dir_all, remove_dir_all};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

/// A simple CLI tool to convert GARbro archives to PNG images.
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long, env = "GARBRO_ROOT")]
    bin_path: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert a single UI file using GARbro GUI.
    Ui {
        /// Path to the UI file to be converted.
        file: PathBuf,
    },
    /// Perform an image conversion operation.
    Ic,
    /// Convert all files with a specified extension in the given root paths to PNG images.
    All {
        /// List of root paths to search for files.
        root_paths: Vec<PathBuf>,
        /// File extension of the files to be converted (default: dpak).
        #[arg(short, long, default_value = "dpak")]
        extension: String,
        /// Output directory for the converted files (default: ./output).
        #[arg(short, long, default_value = "./output")]
        output_dir: PathBuf,
        /// Enable verbose output.
        #[arg(short, long, action)]
        verbose: bool,
    },
    /// Convert files in the given root paths to PNG images, using a top-level approach.
    Top {
        /// List of root paths to search for files.
        root_paths: Vec<PathBuf>,
        /// Output directory for the converted files (default: ./).
        #[arg(short, long, default_value = "./")]
        output_dir: PathBuf,
        /// Enable verbose output.
        #[arg(short, long, action)]
        verbose: bool,
    },
}


fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config = CommandConfig::new(cli.bin_path);

    match &cli.command {
        Commands::Ui { file } => exct_ui(&config, file)?,
        Commands::Ic => exct_ic(&config)?,
        Commands::All {
            root_paths,
            extension,
            output_dir,
            verbose,
        } => exct_all(&config, root_paths, extension, output_dir, *verbose)?,
        Commands::Top {
            root_paths,
            output_dir,
            verbose,
        } => to_png(&config, root_paths, output_dir, *verbose)?,
    }

    Ok(())
}

struct CommandConfig {
    barbro_root: PathBuf,
    img_cmd: &'static str,
    gui_cmd: &'static str,
    csl_cmd: &'static str,
}


impl CommandConfig {
    pub fn new(root: PathBuf) -> Self {
        CommandConfig {
            barbro_root: root,
            ..CommandConfig::default()
        }
    }
    fn img_cmd_full(&self) -> String {
        self.barbro_root.join(self.img_cmd).to_string_lossy().to_string()
    }

    fn gui_cmd_full(&self) -> String {
        self.barbro_root.join(self.gui_cmd).to_string_lossy().to_string()
    }

    fn csl_cmd_full(&self) -> String {
        self.barbro_root.join(self.csl_cmd).to_string_lossy().to_string()
    }
}

impl Default for CommandConfig {
    fn default() -> Self {
        CommandConfig {
            barbro_root: PathBuf::new(),
            img_cmd: "Image.Convert.exe",
            gui_cmd: "GARbro.GUI.exe",
            csl_cmd: "GARbro.Console.exe",
        }
    }
}

fn exct_ui(config: &CommandConfig, file: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Command::new(config.gui_cmd_full())
        .arg(file)
        .spawn()?;
    Ok(())
}

fn exct_ic(config: &CommandConfig) -> Result<(), Box<dyn std::error::Error>> {
    Command::new(config.img_cmd_full()).spawn()?;
    Ok(())
}

fn exct_all(
    config: &CommandConfig,
    root_paths: &[PathBuf],
    extension: &str,
    output_dir: &Path,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    create_dir_all(output_dir)?;
    let temp_dir = output_dir.join("temp");
    create_dir_all(&temp_dir)?;

    _un_comp(config, root_paths, extension, &temp_dir, verbose)?;
    _png_conv(config, &temp_dir, output_dir, verbose)?;
    remove_dir_all(&temp_dir)?;

    Ok(())
}

fn _un_comp(
    config: &CommandConfig,
    root_paths: &[PathBuf],
    extension: &str,
    temp_dir: &Path,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut compressed_files = vec![];

    for dir_path in root_paths {
        for entry in WalkDir::new(dir_path) {
            let entry = entry?;
            if entry.file_type().is_file() && entry.path().extension().unwrap_or_default() == extension {
                compressed_files.push(entry.into_path());
            }
        }
    }

    if compressed_files.is_empty() {
        println!("No compressed files with extension {} found.", extension);
        return Ok(());
    }

    let pb = ProgressBar::new(compressed_files.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.green/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    for f in compressed_files {
        let output = Command::new(config.csl_cmd_full())
            .args(["-x", f.to_str().unwrap()])
            .current_dir(temp_dir)
            .output()?;

        if !output.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
            return Err(format!("Failed to uncompress {}", f.display()).into());
        }

        if verbose {
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }

        pb.inc(1);
    }

    pb.finish();

    Ok(())
}

fn _png_conv(
    config: &CommandConfig,
    source_dir: &Path,
    output_dir: &Path,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file());

    let (raw_pictures, not_convert): (Vec<_>, Vec<_>) = entries
        .partition(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map_or(true, |ext| !matches!(ext, "jpg" | "png" | "jpeg"))
        });


    if raw_pictures.is_empty() {
        println!("No raw pictures found in {:?}", source_dir);
        return Ok(());
    }

    let pb = ProgressBar::new(raw_pictures.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.green/blue}] {pos}/{len} ({eta})")
        .unwrap()
        .progress_chars("#>-"));

    for raw_pic in raw_pictures {
        let result = Command::new(config.img_cmd_full())
            .args(["-t", "PNG", raw_pic.path().to_str().unwrap()])
            .current_dir(output_dir)
            .output()?;

        if !result.status.success() {
            eprintln!("{}", String::from_utf8_lossy(&result.stderr));
            return Err(format!("Failed to convert {}", raw_pic.path().display()).into());
        }

        if verbose {
            println!("{}", String::from_utf8_lossy(&result.stdout));
        }

        pb.inc(1);
    }

    pb.finish();

    for pic in not_convert {
        if pic.path().parent() != Some(output_dir) {
            copy(&pic.path(), output_dir)?;
        }
    }

    Ok(())
}

fn to_png(
    config: &CommandConfig,
    root_paths: &[PathBuf],
    output_dir: &Path,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    for d_path in root_paths {
        _png_conv(config, d_path, output_dir, verbose)?;
    }

    Ok(())
}



