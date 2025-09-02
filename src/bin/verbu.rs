use clap::{Parser, ValueEnum};
use colored::*;
use git2::{Repository, StatusOptions};
use glob::glob;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use semver::{BuildMetadata, Prerelease, Version};
use std::fs;
use std::io::Read;
use std::path::Path;
use fs_extra::file::write_all;
use serde_json::Map;
use toml_edit::{DocumentMut, Item};

/// Supported project types
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum ProjectType {
    /// Python project with pyproject.toml
    Python,
    /// Rust project with Cargo.toml
    Cargo,
    /// Node.js project with package.json
    Node,
    /// Auto-detect project type
    Auto,
}

/// Project configuration for different project types
#[derive(Clone)]
pub(crate) struct ProjectConfig {
    /// File name to look for
    pub(crate) file_name: &'static str,
    /// Path to version field in the configuration
    pub(crate) version_path: Vec<&'static str>,
}

impl ProjectConfig {
    fn for_type(project_type: ProjectType) -> Option<Self> {
        match project_type {
            ProjectType::Python => Some(ProjectConfig {
                file_name: "pyproject.toml",
                version_path: vec!["project", "version"],
            }),
            ProjectType::Cargo => Some(ProjectConfig {
                file_name: "Cargo.toml",
                version_path: vec!["package", "version"],
            }),
            ProjectType::Node => Some(ProjectConfig {
                file_name: "package.json",
                version_path: vec!["version"],
            }),
            ProjectType::Auto => None, // Auto-detection handled separately
        }
    }

    fn detect(project_path: &Path) -> Option<(ProjectType, Self)> {
        let configs = [
            (
                ProjectType::Python,
                ProjectConfig::for_type(ProjectType::Python),
            ),
            (
                ProjectType::Cargo,
                ProjectConfig::for_type(ProjectType::Cargo),
            ),
            (
                ProjectType::Node,
                ProjectConfig::for_type(ProjectType::Node),
            ),
        ];

        for (project_type, config_opt) in configs {
            if let Some(config) = config_opt {
                if project_path.join(config.file_name).exists() {
                    return Some((project_type, config));
                }
            }
        }
        None
    }
}

/// Increment patch version number.
pub fn increment_patch(version: &mut Version) {
    version.patch += 1;
}

/// Increment minor version number and reset patch to 0.
pub fn increment_minor(version: &mut Version) {
    version.minor += 1;
    version.patch = 0;
}

/// Increment major version number and reset minor and patch to 0.
pub fn increment_major(version: &mut Version) {
    version.major += 1;
    version.minor = 0;
    version.patch = 0;
}

/// Command line arguments definition
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "A CLI tool to bump version in project configuration files for multiple projects, with glob support.",
    long_about = None
)]
struct Args {
    /// Paths or glob patterns to project directories
    #[arg(default_value = ".", num_args = 0..)]
    projects: Vec<String>,

    /// Use -i to control bump type:
    /// (none): dev+1 (or make release if -r is present)
    /// -i: patch+1
    /// -ii: minor+1
    /// -iii: major+1
    #[arg(short = 'i', action = clap::ArgAction::Count)]
    bump_level: u8,

    /// Make the version a release version (removes pre-release and build metadata)
    #[arg(short = 'r', long, default_value_t = false)]
    release: bool,

    /// Bump version only if there are git changes in the project directory
    #[arg(short, long, default_value_t = false)]
    git_aware: bool,

    /// File extensions to watch for changes in git-aware mode (comma-separated)
    #[arg(
        short = 'w',
        long,
        env = "WATCH_EXTENSIONS",
    )]
    watch_extensions: Option<String>,

    /// Project type to process
    #[arg(
        short = 't',
        long,
        value_enum,
        default_value = "auto",
        env = "PROJECT_TYPE",
        help = "Project type to process (python, cargo, node, auto)"
    )]
    project_type: ProjectType,
}

/// Discovers the git repository for the given project path.
fn discover_repository(project_path: &Path) -> Result<Repository, Box<dyn std::error::Error>> {
    Repository::discover(project_path).or_else(|_| {
        Repository::discover(".").map_err(|e| {
            format!(
                "Failed to discover git repository. Ensure you are in a git repository and project paths are correct. Error: {}",
                e
            )
                .into()
        })
    })
}

/// Gets the relative path of the project within the git repository.
fn get_relative_project_path(
    project_path: &Path,
    repo: &Repository,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let repo_workdir_raw = repo
        .workdir()
        .ok_or("Git repository is bare, cannot check for changes.")?;

    let canonical_project_path = project_path.canonicalize().map_err(|e| {
        format!(
            "Failed to canonicalize project path '{}': {}. Ensure path is valid.",
            project_path.display(),
            e
        )
    })?;

    let canonical_repo_workdir = repo_workdir_raw.canonicalize().map_err(|e| {
        format!(
            "Failed to canonicalize repository workdir '{}': {}",
            repo_workdir_raw.display(),
            e
        )
    })?;

    canonical_project_path
        .strip_prefix(&canonical_repo_workdir)
        .map(|p| p.to_path_buf())
        .map_err(|_| {
            format!(
                "Project path '{}' (resolved to '{}') is not inside the git repository workdir '{}' (resolved to '{}'). Ensure the project path is a subdirectory of the repository.",
                project_path.display(),
                canonical_project_path.display(),
                repo_workdir_raw.display(),
                canonical_repo_workdir.display()
            )
                .into()
        })
}

/// Checks if a file has one of the allowed extensions.
fn has_allowed_extension(file_path: &str, allowed_extensions: &[String]) -> bool {
    if let Some(extension) = Path::new(file_path).extension() {
        if let Some(ext_str) = extension.to_str() {
            return allowed_extensions.iter().any(|allowed| allowed == ext_str);
        }
    }
    false
}

/// Checks if there are any git changes (modified, added, untracked, etc.)
/// within the specified project path, filtering by allowed file extensions.
fn has_git_changes_in_path(
    project_path: &Path,
    allowed_extensions: &[String],
) -> Result<bool, Box<dyn std::error::Error>> {
    let repo = discover_repository(project_path)?;
    let relative_project_path = get_relative_project_path(project_path, &repo)?;

    let mut status_opts = StatusOptions::new();
    status_opts.include_untracked(true);
    status_opts.recurse_untracked_dirs(true);

    if !relative_project_path.as_os_str().is_empty() {
        status_opts.pathspec(&relative_project_path);
    }

    let statuses = repo.statuses(Some(&mut status_opts))?;

    for entry in statuses.iter() {
        if let Some(file_path) = entry.path() {
            if has_allowed_extension(file_path, allowed_extensions) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Processes a single project directory.
fn process_project_directory(
    project_path: &Path,
    bump_level: u8,
    release: bool,
    git_aware: bool,
    allowed_extensions: &[String],
    project_type: ProjectType,
) -> Result<bool, Box<dyn std::error::Error>> {
    let (detected_type, config) = match project_type {
        ProjectType::Auto => match ProjectConfig::detect(project_path) {
            Some((t, c)) => (t, c),
            None => {
                eprintln!(
                    "{} {}: No supported project configuration file found in {}. Skipping.",
                    "⚠️".yellow(),
                    "Skipping".yellow(),
                    project_path.display().to_string().cyan()
                );
                return Ok(false);
            }
        },
        specific_type => match ProjectConfig::for_type(specific_type) {
            Some(config) => {
                let config_path = project_path.join(config.file_name);
                if !config_path.exists() {
                    eprintln!(
                        "{} {}: {} not found in {}. Skipping.",
                        "⚠️".yellow(),
                        "Skipping".yellow(),
                        config.file_name.cyan(),
                        project_path.display().to_string().cyan()
                    );
                    return Ok(false);
                }
                (specific_type, config)
            }
            None => {
                eprintln!(
                    "{} {}: Project type {:?} is not supported. Skipping.",
                    "⚠️".yellow(),
                    "Skipping".yellow(),
                    specific_type
                );
                return Ok(false);
            }
        },
    };

    let config_path = project_path.join(config.file_name);

    if git_aware {
        match has_git_changes_in_path(project_path, allowed_extensions) {
            Ok(true) => {
                println!(
                    "{} {}: Git changes detected in {} (watching: {}). Proceeding.",
                    "✅".green(),
                    "Info".green(),
                    project_path.display().to_string().cyan(),
                    allowed_extensions.join(",").yellow()
                );
            }
            Ok(false) => {
                println!(
                    "{} {}: No git changes detected in {} for watched extensions ({}). Skipping version bump (git-aware mode).",
                    "ℹ️".blue(),
                    "Skipping".blue(),
                    project_path.display().to_string().cyan(),
                    allowed_extensions.join(",").yellow()
                );
                return Ok(false);
            }
            Err(e) => {
                eprintln!(
                    "{} {}: Error checking git status for {}: {}. Aborting bump for this project.",
                    "❌".red(),
                    "Error".red(),
                    project_path.display().to_string().cyan(),
                    e.to_string().red()
                );
                return Err(format!("Error checking git status: {}", e).into());
            }
        }
    }

    println!(
        "{} {} ({:?}): {}",
        "⚙️".blue(),
        "Processing".blue(),
        detected_type,
        project_path.display().to_string().cyan()
    );

    bump_version(
        config_path.to_str().ok_or_else(|| {
            format!(
                "Path {} contains non-UTF8 characters",
                config_path.display()
            )
        })?,
        bump_level,
        release,
        &config,
    )?;
    Ok(true)
}

/// Processes projects found via a single glob pattern.
fn process_glob_pattern(
    project_glob_pattern: &str,
    bump_level: u8,
    release: bool,
    git_aware: bool,
    allowed_extensions: &[String],
    project_type: ProjectType,
) -> Result<bool, Box<dyn std::error::Error>> {
    let entries = match glob(project_glob_pattern) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!(
                "{} {}: Invalid glob pattern '{}': {}. Skipping this pattern.",
                "❌".red(),
                "Error".red(),
                project_glob_pattern.yellow(),
                e.to_string().red()
            );
            return Ok(false);
        }
    };

    let mut found_paths_for_pattern = false;
    let paths: Vec<_> = entries
        .filter_map(|entry| match entry {
            Ok(p) => {
                if p.is_dir() {
                    found_paths_for_pattern = true;
                    Some(p)
                } else {
                    None
                }
            }
            Err(e) => {
                eprintln!(
                    "{} {}: Error accessing path from glob pattern '{}': {}. Skipping this item.",
                    "❌".red(),
                    "Error".red(),
                    project_glob_pattern.yellow(),
                    e.to_string().red()
                );
                None
            }
        })
        .collect();

    if paths.is_empty() {
        if !found_paths_for_pattern {
            eprintln!(
                "{} {}: No directories found matching glob pattern '{}'",
                "⚠️".yellow(),
                "Warning".yellow(),
                project_glob_pattern.yellow()
            );
        }
        return Ok(false);
    }

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["-", "\\", "|", "/"])
            .template("{spinner} {wide_msg}")
            .unwrap(),
    );
    pb.set_message("Processing projects...");
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    let results: Vec<_> = paths
        .into_par_iter()
        .progress_with(pb)
        .map(|path| {
            match process_project_directory(
                &path,
                bump_level,
                release,
                git_aware,
                allowed_extensions,
                project_type,
            ) {
                Ok(true) => Some(true),
                Ok(false) => None,
                Err(e) => {
                    eprintln!("{}", e);
                    panic!("{}", e);
                }
            }
        })
        .collect();

    Ok(!results.is_empty())
}

/// Main function
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut processed_any_project_overall = false;

    let default_extensions = match args.project_type {
        ProjectType::Python => "py",
        ProjectType::Cargo => "rs",
        ProjectType::Node => "js,ts,jsx,tsx,json",
        ProjectType::Auto => "py,rs,js,ts,jsx,tsx,json",
    };

    let allowed_extensions: Vec<String> = args
        .watch_extensions
        .as_deref()
        .unwrap_or(default_extensions)
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    for project_glob_pattern in &args.projects {
        match process_glob_pattern(
            project_glob_pattern,
            args.bump_level,
            args.release,
            args.git_aware,
            &allowed_extensions,
            args.project_type,
        ) {
            Ok(processed) => {
                if processed {
                    processed_any_project_overall = true;
                }
            }
            Err(e) => {
                return Err(format!(
                    "Aborted due to error while processing pattern '{}'. Original error: {}",
                    project_glob_pattern.yellow(),
                    e
                )
                    .into());
            }
        }
    }

    if !processed_any_project_overall {
        let mut error_message = "No project versions were bumped.".to_string();
        let is_default_single_project_dot = args.projects.len() == 1 && args.projects[0] == ".";
        if is_default_single_project_dot {
            error_message.push_str(
                " No supported project configuration file found in the current directory.",
            );
        } else if args.git_aware {
            error_message.push_str(&format!(
                " In git-aware mode, this can also occur if no targeted projects had relevant git changes for watched extensions ({}).",
                allowed_extensions.join(",")
            ));
        }
        error_message
            .push_str(" Please check paths, glob patterns, and ensure a supported project configuration file exists.");

        eprintln!("{} {}", "❌".red(), error_message.red());
        return Err(error_message.into());
    }

    Ok(())
}

/// Bumps the version of the specified project.
fn bump_version(
    config_path: &str,
    level: u8,
    release_mode: bool,
    config: &ProjectConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(config_path);
    let version_str = if path.extension().and_then(|s| s.to_str()) == Some("json") {
        get_version_from_json(config_path, &config.version_path)?
    } else {
        let contents = fs::read_to_string(config_path)?;
        let doc = contents.parse::<DocumentMut>()?;
        get_version_from_toml(&doc, &config.version_path)?
            .to_string()
    };

    let mut version = Version::parse(&version_str)?;
    let old_version_str = version.to_string();

    match level {
        1 => increment_patch(&mut version),
        2 => increment_minor(&mut version),
        3 => increment_major(&mut version),
        0 => {}
        _ => {
            eprintln!(
                "{} {}: Too many -i flags: use up to 3",
                "❌".red(),
                "Error".red()
            );
            return Err("Too many -i flags: use up to 3".into());
        }
    }

    if release_mode {
        version.pre = Prerelease::EMPTY;
    } else {
        if level == 0 {
            bump_dev(&mut version)?;
        } else {
            version.pre = Prerelease::new("dev0").expect("Valid prerelease identifier");
        }
    }

    if level > 0 || release_mode {
        version.build = BuildMetadata::EMPTY;
    }

    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        update_version_in_json(config_path, version.to_string(), &config.version_path)?;
    } else {
        let mut doc = fs::read_to_string(config_path)?.parse::<DocumentMut>()?;
        update_version_in_toml(&mut doc, version.to_string(), &config.version_path)?;
        fs::write(config_path, doc.to_string())?;
    }

    println!(
        "{} {} in {} from {} to {}",
        "🎉".green(),
        "Bumped version".green(),
        config_path.cyan(),
        old_version_str.yellow(),
        version.to_string().bold().green()
    );

    Ok(())
}

/// Extracts version from TOML
fn get_version_from_toml<'a>(
    doc: &'a DocumentMut,
    version_path: &[&str],
) -> Result<&'a str, Box<dyn std::error::Error>> {
    let mut current = doc.as_item();

    for (i, &key) in version_path.iter().enumerate() {
        current = current.get(key).ok_or_else(|| {
            let path_so_far = version_path[..=i].join(".");
            format!("'{}' not found in configuration file", path_so_far)
        })?;
    }

    current.as_str().ok_or_else(|| {
        format!(
            "Version at path '{}' is not a string",
            version_path.join(".")
        )
            .into()
    })
}

/// Updates version in TOML
fn update_version_in_toml(
    doc: &mut DocumentMut,
    new_version: String,
    version_path: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    if version_path.is_empty() {
        return Err("Version path cannot be empty".into());
    }

    let mut current = doc.as_table_mut();

    for &key in &version_path[..version_path.len() - 1] {
        current = current
            .get_mut(key)
            .and_then(Item::as_table_mut)
            .ok_or_else(|| format!("'{}' table not found in configuration file", key))?;
    }

    let version_key = version_path.last().ok_or_else(
        || format!(
            "Version key '{}' not found in {}",
            version_path.last().unwrap(),
            version_path[..version_path.len() - 1].join(".")
        )
    )?;
    current[version_key] = toml_edit::value(new_version);

    Ok(())
}

/// Reads version from JSON
fn get_version_from_json(
    config_path: &str,
    version_path: &[&str],
) -> Result<String, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(config_path)?;
    let value: serde_json::Value = serde_json::from_str(&contents)?;

    let mut current = &value;
    for &key in version_path {
        current = current
            .get(key)
            .ok_or_else(|| format!("Key '{}' not found in {}", key, config_path))?;
    }

    current
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Version field is not a string".into())
}

/// Updates version in JSON and writes back with formatting
fn update_version_in_json(
    config_path: &str,
    new_version: String,
    version_path: &[&str],
) -> Result<(), Box<dyn std::error::Error>> {
    let file =fs::File::open(config_path)?;
    let mut buf_reader = std::io::BufReader::new(&file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;

    let mut data: Map<String, serde_json::Value> = serde_json::from_str(&contents)?;

    let mut current = &mut data;
    for &key in &version_path[..version_path.len() - 1] {
        current = current
            .get_mut(key)
            .and_then(|v| v.as_object_mut())
            .ok_or_else(|| format!("Intermediate key '{}' not found or not an object", key))?;
    }

    let last_key = version_path.last().unwrap();
    *current
        .get_mut(*last_key)
        .ok_or_else(|| format!("Final key '{}' not found", last_key))? =
        serde_json::Value::String(new_version);

    let formatted = serde_json::to_string_pretty(&data)?;
    drop(file);

    write_all(config_path,&formatted)?;
    Ok(())
}

/// Bumps dev version (e.g., dev3 → dev4)
fn bump_dev(version: &mut Version) -> Result<(), Box<dyn std::error::Error>> {
    let pre = &mut version.pre;
    if pre.is_empty() {
        version.patch += 1;
        *pre = Prerelease::new("dev0")?;
    } else if let Some(n_str) = pre.as_str().strip_prefix("dev") {
        if let Ok(n) = n_str.parse::<u64>() {
            *pre = Prerelease::new(&format!("dev{}", n + 1))?;
        } else {
            *pre = Prerelease::new("dev0")?;
        }
    } else {
        *pre = Prerelease::new("dev0")?;
    }
    Ok(())
}