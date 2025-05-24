use clap::Parser;
use colored::*;
// Added for colorful output
use git2::{Repository, StatusOptions};
use glob::glob;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use semver::{BuildMetadata, Prerelease, Version};
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item};

/// Increment patch version number.
/// Note: Pre-release and build metadata are handled by the calling `bump_version` function.
pub fn increment_patch(version: &mut Version) {
    version.patch += 1;
}

/// Increment minor version number and reset patch to 0.
/// Note: Pre-release and build metadata are handled by the calling `bump_version` function.
pub fn increment_minor(version: &mut Version) {
    version.minor += 1;
    version.patch = 0;
}

/// Increment major version number and reset minor and patch to 0.
/// Note: Pre-release and build metadata are handled by the calling `bump_version` function.
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
    about = "A CLI tool to bump version in pyproject.toml for multiple projects, with glob support.",
    long_about = None
)]
struct Args {
    /// Paths or glob patterns to project directories, each must contain pyproject.toml
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
    #[arg(
        short,
        long,
        default_value_t = false,
        help = "Bump version only if there are git changes in the project directory"
    )]
    git_aware: bool,
    /// File extensions to watch for changes in git-aware mode (comma-separated)
    #[arg(
        short = 'w',
        long,
        default_value = "py,rs",
        env = "WATCH_EXTENSIONS",
        help = "File extensions to watch for changes in git-aware mode (comma-separated, e.g., 'py,rs,js')"
    )]
    watch_extensions: String,
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

    // Canonicalize project_path to get an absolute, normalized path.
    let canonical_project_path = project_path.canonicalize().map_err(|e| {
        format!(
            "Failed to canonicalize project path '{}': {}. Ensure path is valid.",
            project_path.display(),
            e
        )
    })?;

    // Canonicalize repo_workdir as well to ensure consistent path formats for strip_prefix,
    // especially for handling `\\?\` prefixes on Windows.
    let canonical_repo_workdir = repo_workdir_raw.canonicalize().map_err(|e| {
        format!(
            "Failed to canonicalize repository workdir '{}': {}",
            repo_workdir_raw.display(),
            e
        )
    })?;

    // Get project_path relative to the repository's workdir.
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

    // If relative_project_path is empty, it means project_path is the repo root.
    // In this case, we don't set a pathspec, so it checks all files in the repo.
    if !relative_project_path.as_os_str().is_empty() {
        status_opts.pathspec(&relative_project_path);
    }

    let statuses = repo.statuses(Some(&mut status_opts))?;

    // Check if any changed files have allowed extensions
    for entry in statuses.iter() {
        if let Some(file_path) = entry.path() {
            if has_allowed_extension(file_path, allowed_extensions) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Processes a single project directory: verifies `pyproject.toml` existence, and calls `bump_version`.
/// Returns `Ok(true)` if version was bumped, `Ok(false)` if skipped,
/// or `Err` for critical errors.
fn process_project_directory(
    project_path: &Path,
    bump_level: u8,
    release: bool,
    git_aware: bool,
    allowed_extensions: &[String],
) -> Result<bool, Box<dyn std::error::Error>> {
    let pyproject_path = project_path.join("pyproject.toml");
    if !pyproject_path.exists() {
        eprintln!(
            "{} {}: pyproject.toml not found in {}. Skipping.",
            "⚠️".yellow(),
            "Skipping".yellow(),
            project_path.display().to_string().cyan()
        );
        return Ok(false); // Skipped due to missing pyproject.toml
    }

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
                return Ok(false); // Skipped due to no git changes
            }
            Err(e) => {
                eprintln!(
                    "{} {}: Error checking git status for {}: {}. Aborting bump for this project.",
                    "❌".red(),
                    "Error".red(),
                    project_path.display().to_string().cyan(),
                    e.to_string().red()
                );
                return Err(format!(
                    "Error checking git status for {}: {}",
                    project_path.display(),
                    e
                )
                .into());
            }
        }
    }

    println!(
        "{} {}: {}",
        "⚙️".blue(),
        "Processing".blue(),
        project_path.display().to_string().cyan()
    );
    bump_version(
        pyproject_path.to_str().ok_or_else(|| {
            format!(
                "Path {} contains non-UTF8 characters",
                pyproject_path.display()
            )
        })?,
        bump_level,
        release,
    )?;
    Ok(true) // Successfully processed and bumped version
}

/// Processes projects found via a single glob pattern.
/// Returns Ok(bool) indicating if any project was processed under this pattern,
/// or Err for critical errors that should halt all operations.
fn process_glob_pattern(
    project_glob_pattern: &str,
    bump_level: u8,
    release: bool,
    git_aware: bool,
    allowed_extensions: &[String],
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
            return Ok(false); // Not a critical error for the whole app, just this pattern.
        }
    };

    let mut found_paths_for_pattern = false;

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["-", "\\", "|", "/"])
            .template("{spinner} {wide_msg}")
            .unwrap(),
    );

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

/// Main function entry point
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut processed_any_project_overall = false;

    // Parse allowed extensions from the command line argument
    let allowed_extensions: Vec<String> = args
        .watch_extensions
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
        ) {
            Ok(processed_this_pattern) => {
                if processed_this_pattern {
                    processed_any_project_overall = true;
                }
            }
            Err(e) => {
                // The specific error message should have already been printed with colors.
                // Return an error that indicates context, preserving the original error cause.
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
        if is_default_single_project_dot && !Path::new("pyproject.toml").exists() {
            error_message = "No 'pyproject.toml' found in the current directory.".to_string();
        } else if args.git_aware {
            error_message.push_str(&format!(" In git-aware mode, this can also occur if no targeted projects had relevant git changes for watched extensions ({}).", allowed_extensions.join(",")));
        }
        error_message.push_str(" Please check paths, glob patterns, and ensure 'pyproject.toml' exists in target project directories.");

        eprintln!("{} {}", "❌".red(), error_message.red());
        // Return the error message; it will be printed by Rust's default error handler if main returns Err.
        return Err(error_message.into());
    }

    Ok(())
}

/// Bumps the version of the specified project.
///
/// # Arguments
///
/// * `pyproject_path` - Path to the `pyproject.toml` file.
/// * `level` - Bump level (0~3), corresponding to dev/patch/minor/major.
/// * `release_mode` - If true, makes the version a release version (removes pre-release/build).
pub fn bump_version(
    pyproject_path: &str,
    level: u8,
    release_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(pyproject_path)?;
    let mut doc = contents.parse::<DocumentMut>()?;

    let version_str = get_version_from_toml(&doc)?;
    let mut version = Version::parse(version_str)?;
    let old_version_str = version.to_string(); // For logging

    // Step 1: Apply numeric increment if level > 0
    match level {
        1 => increment_patch(&mut version),
        2 => increment_minor(&mut version),
        3 => increment_major(&mut version),
        0 => {} // No numeric change based on level itself for this step
        _ => {
            eprintln!(
                "{} {}: Too many -i flags: use up to 3",
                "❌".red(),
                "Error".red()
            );
            return Err("Too many -i flags: use up to 3".into());
        }
    }

    // Step 2: Set pre-release identifier
    if release_mode {
        // If -r is specified, it's a release version, clear pre-release.
        version.pre = Prerelease::EMPTY;
    } else {
        // Not a release version.
        if level == 0 {
            // Default action (no -i flags), bump dev.
            // bump_dev handles its own pre-release logic.
            bump_dev(&mut version)?;
        } else {
            // Bumped patch, minor, or major (-i, -ii, -iii) and not -r.
            // Instruction: "bump到新版本的时候一定是-dev0"
            // (When bumping to a new version, it must be -dev0)
            version.pre =
                Prerelease::new("dev0").expect("Static string 'dev0' should be valid prerelease");
        }
    }

    // Step 3: Set build metadata.
    // Clear build metadata if we bumped a component (level > 0) or if making a release (release_mode is true).
    // If only bumping dev (level is 0 and not release_mode), preserve bump_dev's behavior regarding build metadata
    // (bump_dev itself does not modify build metadata).
    if level > 0 || release_mode {
        version.build = BuildMetadata::EMPTY;
    }

    update_version_in_toml(&mut doc, version.to_string())?;
    fs::write(pyproject_path, doc.to_string())?;
    println!(
        "{} {} in {} from {} to {}",
        "🎉".green(),
        "Bumped version".green(),
        pyproject_path.cyan(),
        old_version_str.yellow(),
        version.to_string().bold().green()
    );
    Ok(())
}

/// Extracts the version string from the TOML document.
fn get_version_from_toml(doc: &DocumentMut) -> Result<&str, Box<dyn std::error::Error>> {
    doc.get("project")
        .and_then(|p| p.get("version"))
        .and_then(Item::as_str)
        .ok_or_else(|| "Version not found or invalid in pyproject.toml".into())
}

/// Updates the version field in the TOML document.
fn update_version_in_toml(
    doc: &mut DocumentMut,
    new_version: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let project_table = doc
        .get_mut("project")
        .and_then(Item::as_table_mut)
        .ok_or_else(|| {
            Box::<dyn std::error::Error>::from("'project' table not found in pyproject.toml")
        })?;

    project_table["version"] = toml_edit::value(new_version);
    Ok(())
}

/// Bumps the dev version (e.g., dev3 -> dev4).
/// If the version has no pre-release, it becomes dev0.
/// If it has a non-dev pre-release, it becomes dev0.
fn bump_dev(version: &mut Version) -> Result<(), Box<dyn std::error::Error>> {
    let pre = &mut version.pre;
    if pre.is_empty() {
        *pre = Prerelease::new("dev0")?;
    } else if let Some(n_str) = pre.as_str().strip_prefix("dev") {
        if let Ok(n) = n_str.parse::<u64>() {
            *pre = Prerelease::new(&format!("dev{}", n + 1))?;
        } else {
            // Handles cases like "devX" where X is not a number, or "dev" itself.
            // Treat as a new "dev0" sequence.
            *pre = Prerelease::new("dev0")?;
        }
    } else {
        // If it's some other pre-release (e.g., "alpha1", "rc2"), convert it to "dev0".
        *pre = Prerelease::new("dev0")?;
    }
    // Build metadata is not touched by bump_dev by design.
    // The calling function `bump_version` decides on handling build metadata globally.
    Ok(())
}
