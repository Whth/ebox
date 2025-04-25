use clap::{Parser, Subcommand};
use dirs::data_dir;
use glob::glob;
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
/// A command line tool to manage Factorio mods by moving old versions to an 'old_mods' directory.
/// This tool helps in organizing your Factorio mods by archiving older versions to a separate directory.
#[command(author, version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // Existing commands...
    /// Move old mods to an 'old_mods' directory
    #[command(alias = "m")]
    Move {
        /// The path to the mods directory
        #[arg(short, long, default_value = get_default_mods_dir())]
        mods_dir: PathBuf,

        /// The output directory for old mods
        #[arg(short, long, default_value = get_default_old_mods_dir())]
        output_dir: PathBuf,
    },

    /// Export enabled mods as a zip file
    #[command(alias = "e")]
    Export {
        /// The path to the mods directory
        #[arg(short, long, default_value= get_default_mods_dir())]
        mods_dir: PathBuf,

        /// The output zip file path
        #[arg(short, long, default_value = "./enabled_mods.zip")]
        output_zip: PathBuf,

        /// Include the mod-settings.dat file in the zip
        #[arg(short, long, default_value_t = false)]
        include_settings: bool,
    },

    /// Import mods from a zip file to the mods directory
    #[command(alias = "i")]
    Import {
        /// The input zip file path
        #[arg(short, long)]
        input_zip: PathBuf,

        /// The path to the mods directory
        #[arg(short, long, default_value = get_default_mods_dir())]
        mods_dir: PathBuf,
    },

    /// Install a mod from a local path or URL
    #[command(alias = "in")]
    Install {
        /// The path or URL to the mod zip file
        source: String,

        /// The path to the mods directory
        #[arg(short, long, default_value = get_default_mods_dir())]
        mods_dir: PathBuf,
    },

    /// List out the installed mods
    #[command(alias = "l")]
    List {
        /// The path to the mods directory
        #[arg(short, long, default_value = get_default_mods_dir())]
        mods_dir: PathBuf,
    },
}

fn get_default_mods_dir() -> String {
    let mut mods_dir = data_dir().unwrap();
    mods_dir.push("Factorio");
    mods_dir.push("mods");
    mods_dir.to_string_lossy().to_string()
}
fn get_default_old_mods_dir() -> String {
    let mut old_mods_dir = data_dir().unwrap();
    old_mods_dir.push("Factorio");
    old_mods_dir.push("old_mods");
    old_mods_dir.to_string_lossy().to_string()
}

// 新增: 提取正则表达式匹配逻辑到 ModEntry 结构体
#[derive(Clone)] // 添加 Clone 特性
struct ModEntry {
    base_name: String,
    version: String,
    source_path: PathBuf, // Added field for the original zip file source path
}

impl ModEntry {
    fn from_file_name(file_name: &str) -> Option<Self> {
        let re = Regex::new(r"^(.*)_(\d+\.\d+\.\d+)\.zip$").ok()?;
        let caps = re.captures(file_name)?;
        Some(ModEntry {
            base_name: caps.get(1)?.as_str().to_string(),
            version: caps.get(2)?.as_str().to_string(),
            source_path: PathBuf::from(file_name), // Initialize the source_path with the file name
        })
    }
}

trait RetainLatest {
    fn retain_latest(&self) -> Vec<ModEntry>;
}

impl RetainLatest for Vec<ModEntry> {
    fn retain_latest(&self) -> Vec<ModEntry> {
        // 按 base_name 分组
        let mut grouped: HashMap<String, Vec<&ModEntry>> = HashMap::new();
        for entry in self {
            grouped
                .entry(entry.base_name.clone())
                .or_insert_with(Vec::new)
                .push(entry);
        }

        // 在每个分组中选择版本号最高的 ModEntry
        let mut latest_entries: Vec<ModEntry> = Vec::new();
        for entries in grouped.values() {
            if let Some(latest) = entries.iter().max_by(|a, b| a.version.cmp(&b.version)) {
                latest_entries.push(latest.to_owned().clone());
            }
        }

        latest_entries
    }
}

// 修改: 将 get_mod_entries 函数拆分为更小的函数
fn get_mod_entries(mods_path: &PathBuf) -> Result<Vec<ModEntry>, Box<dyn std::error::Error>> {
    let pattern = format!("{}/*.zip", mods_path.display());
    let entries = glob(&pattern)
        .expect("Failed to read mods directory")
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            entry
                .file_name()
                .and_then(|f| f.to_str())
                .and_then(|s| ModEntry::from_file_name(s))
        })
        .collect();

    Ok(entries)
}

// 修改: 更新 get_latest_versions 函数以适配新的 get_mod_entries 返回值
fn get_latest_versions(
    mods_path: &PathBuf,
) -> Result<HashMap<String, (PathBuf, u64)>, Box<dyn std::error::Error>> {
    let mod_entries = get_mod_entries(mods_path)?;

    // 使用 RetainLatest trait 的 retain_latest 方法筛选最新版本
    let latest_entries = mod_entries.retain_latest();

    let latest_versions: HashMap<String, (PathBuf, u64)> = latest_entries
        .into_iter()
        .filter_map(|mod_entry| {
            let entry = mod_entry.source_path.clone(); // 使用 ModEntry 中的 source_path
            let metadata = fs::metadata(&entry).ok()?;
            let modified_time = metadata
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();
            Some((mod_entry.base_name, (entry, modified_time)))
        })
        .collect();

    Ok(latest_versions)
}

// 修改: 更新 move_old_mods 函数以适配新的 get_mod_entries 返回值
fn move_old_mods(
    mods_path: &PathBuf,
    output_dir: &PathBuf,
    latest_versions: HashMap<String, (PathBuf, u64)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mod_entries = get_mod_entries(mods_path)?;

    mod_entries.into_par_iter().for_each(|mod_entry| {
        let entry = mod_entry.source_path.clone(); // 使用 ModEntry 中的 source_path

        // Skip if this is the latest version
        let Some((latest_entry, _)) = latest_versions.get(&mod_entry.base_name) else {
            return;
        };
        if latest_entry == &entry {
            return;
        };

        // Prepare destination path
        let mut dest_path = output_dir.clone();
        dest_path.push(entry.file_name().unwrap());

        // Move the file
        match fs::rename(&entry, &dest_path) {
            Ok(_) => println!("Moved {} to {}", entry.display(), dest_path.display()),
            Err(e) => eprintln!(
                "Failed to move {} to {}: {}",
                entry.display(),
                dest_path.display(),
                e
            ),
        }
    });

    Ok(())
}

fn read_mod_config(
    config_file: &PathBuf,
) -> Result<HashMap<String, bool>, Box<dyn std::error::Error>> {
    let config_content = fs::read_to_string(config_file)?;
    let config: serde_json::Value = serde_json::from_str(&config_content)?;

    let mut mod_config = HashMap::new();

    let mods = match config.get("mods").and_then(|v| v.as_array()) {
        Some(mods_array) => mods_array,
        None => return Ok(mod_config),
    };

    for mod_entry in mods {
        let name = match mod_entry.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };

        let enabled = match mod_entry.get("enabled").and_then(|v| v.as_bool()) {
            Some(e) => e,
            None => continue,
        };

        mod_config.insert(name.to_string(), enabled);
    }

    Ok(mod_config)
}

fn zip_enabled_mods(
    mods_path: &PathBuf,
    output_zip: &PathBuf,
    include_settings: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_file = mods_path.join("mod-list.json");
    let mod_config = read_mod_config(&config_file)?;
    let mod_entries = get_mod_entries(mods_path)?;

    let output_path = output_zip.to_string_lossy();
    let mut files_to_compress = vec![];

    if config_file.exists() {
        files_to_compress.push(config_file.to_string_lossy().to_string());
        println!("Added mod-list.json to {}", output_path);
    } else {
        eprintln!("mod-list.json not found in the mods directory.");
    }

    for mod_entry in mod_entries {
        let entry = mod_entry.source_path;
        if mod_config
            .get(&mod_entry.base_name)
            .copied()
            .unwrap_or(false)
        {
            files_to_compress.push(entry.to_string_lossy().to_string());
            println!("Added {} to {}", entry.display(), output_path);
        }
    }

    if include_settings {
        let settings_file = mods_path.join("mod-settings.dat");
        if settings_file.exists() {
            files_to_compress.push(settings_file.to_string_lossy().to_string());
            println!("Added mod-settings.dat to {}", output_path);
        } else {
            eprintln!("mod-settings.dat not found in the mods directory.");
        }
    }

    // 调用 7z 命令进行压缩
    let status = Command::new("7z")
        .arg("a")
        .arg(output_path.to_string())
        .args(files_to_compress)
        .status()?;

    if !status.success() {
        return Err("Failed to compress enabled mods using 7z".into());
    }

    Ok(())
}

fn import_mods(mods_path: &PathBuf, input_zip: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let input_path = input_zip.to_string_lossy();
    let output_path = mods_path.to_string_lossy();

    // 调用 7z 命令进行解压
    let status = Command::new("7z")
        .arg("x")
        .arg(input_path.to_string())
        .arg("-o")
        .arg(output_path.to_string())
        .status()?;

    if !status.success() {
        return Err("Failed to extract mods using 7z".into());
    }

    println!("Extracted mods from {} to {}", input_path, output_path);
    Ok(())
}

fn package_folder_to_zip(
    folder_path: &PathBuf,
    output_zip: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let output_path = output_zip.to_string_lossy();
    let folder_path = folder_path.to_string_lossy();

    // 调用 7z 命令进行压缩
    let status = Command::new("7z")
        .arg("a")
        .arg(output_path.to_string())
        .arg(folder_path.to_string())
        .status()?;

    if !status.success() {
        return Err("Failed to compress folder using 7z".into());
    }

    println!("Compressed folder {} into {}", folder_path, output_path);
    Ok(())
}

// 新增: 定义 ModSource 结构体
struct ModSource {
    path: PathBuf,
    source_type: ModSourceType,
}

// 新增: 定义 ModSourceType 枚举
enum ModSourceType {
    LocalFile,
    Url,
    Folder,
}

// 新增: 验证 mod 来源的有效性
fn validate_mod_source(source: &str) -> Result<ModSource, Box<dyn std::error::Error>> {
    if source.starts_with("http://") || source.starts_with("https://") {
        // URL 类型
        let file_path = PathBuf::from(source);
        Ok(ModSource {
            path: file_path,
            source_type: ModSourceType::Url,
        })
    } else {
        // 本地文件或文件夹
        let file_path = PathBuf::from(source);
        if !file_path.exists() {
            return Err(format!("File or folder not found: {}", source).into());
        }

        if file_path.is_dir() {
            Ok(ModSource {
                path: file_path,
                source_type: ModSourceType::Folder,
            })
        } else {
            Ok(ModSource {
                path: file_path,
                source_type: ModSourceType::LocalFile,
            })
        }
    }
}

// 新增: 处理文件夹形式的 mod
fn process_mod_folder(
    folder_path: &PathBuf,
    mods_path: &PathBuf,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Check for info.json
    let info_json_path = folder_path.join("info.json");
    if !info_json_path.exists() {
        return Err("info.json not found in the folder".into());
    }

    // Read and validate info.json
    let info_content = fs::read_to_string(&info_json_path)?;
    let info: serde_json::Value = serde_json::from_str(&info_content)?;
    let name = info
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'name' field in info.json")?;
    let version = info
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or("Missing 'version' field in info.json")?;

    // Generate zip file name
    let zip_file_name = format!("{}_{}.zip", name, version);
    let zip_file_path = mods_path.join(zip_file_name);

    // Package folder into zip
    package_folder_to_zip(folder_path, &zip_file_path)?;

    println!(
        "Packaged folder {} into {}",
        folder_path.display(),
        zip_file_path.display()
    );

    Ok(zip_file_path)
}

// 修改: 重构 install_mod 函数
fn install_mod(mods_path: &PathBuf, source: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 验证 mod 来源
    let mod_source = validate_mod_source(source)?;

    // 根据来源类型处理 mod
    let file_path = match mod_source.source_type {
        ModSourceType::Url => {
            // 下载文件
            let response = reqwest::blocking::get(mod_source.path.to_string_lossy().as_ref())?;
            if !response.status().is_success() {
                return Err(format!("Failed to download mod from {}", source).into());
            }

            let file_name = response
                .url()
                .path_segments()
                .and_then(|segments| segments.last())
                .ok_or("Invalid URL or missing file name")?;
            let file_path = mods_path.join(file_name);
            let mut file = fs::File::create(&file_path)?;
            let content = response.bytes()?;
            file.write_all(&content)?;
            file_path
        }
        ModSourceType::LocalFile => mod_source.path.clone(),
        ModSourceType::Folder => process_mod_folder(&mod_source.path, mods_path)?,
    };

    // 验证文件名是否符合 ModEntry 正则表达式
    let file_name = file_path
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or("Invalid file name")?;
    if ModEntry::from_file_name(file_name).is_none() {
        return Err(format!("Invalid mod file name: {}", file_name).into());
    }

    // 移动文件到 mods 目录（如果不在 mods 目录中）
    if file_path.parent().unwrap() != mods_path {
        let dest_path = mods_path.join(file_name);
        fs::rename(&file_path, &dest_path)?;
        println!(
            "Installed {} to {}",
            file_path.display(),
            dest_path.display()
        );
    } else {
        println!("Installed {}", file_path.display());
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Move {
            mods_dir,
            output_dir,
        } => {
            if !mods_dir.exists() || !mods_dir.is_dir() {
                return Err("Mods directory does not exist or is not a directory".into());
            }
            if !output_dir.exists() {
                fs::create_dir(&output_dir)?;
            }

            let latest_versions = get_latest_versions(&mods_dir)?;
            move_old_mods(&mods_dir, &output_dir, latest_versions)?;
        }
        Commands::Export {
            mods_dir,
            output_zip,
            include_settings,
        } => {
            if !mods_dir.exists() || !mods_dir.is_dir() {
                return Err("Mods directory does not exist or is not a directory".into());
            }

            zip_enabled_mods(&mods_dir, &output_zip, include_settings)?;
        }
        Commands::Import {
            input_zip,
            mods_dir,
        } => {
            if !mods_dir.exists() || !mods_dir.is_dir() {
                return Err("Mods directory does not exist or is not a directory".into());
            }

            import_mods(&mods_dir, &input_zip)?;
        }
        Commands::Install { source, mods_dir } => {
            if !mods_dir.exists() || !mods_dir.is_dir() {
                return Err("Mods directory does not exist or is not a directory".into());
            }

            install_mod(&mods_dir, &source)?;
        }
        Commands::List { mods_dir } => {
            if !mods_dir.exists() || !mods_dir.is_dir() {
                return Err("Mods directory does not exist or is not a directory".into());
            }

            let mod_entries = get_mod_entries(&mods_dir)?;
            println!("Installed mods:");
            for entry in mod_entries {
                println!("{} (Version: {})", entry.base_name, entry.version);
            }
        }
    }

    Ok(())
}
