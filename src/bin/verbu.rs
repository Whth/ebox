use clap::Parser;
use colored::*;
// Added for colorful output
use git2::{Repository, StatusOptions};
use glob::glob;
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
}

/// Checks if there are any git changes (modified, added, untracked, etc.)
/// within the specified project path.
fn has_git_changes_in_path(project_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    // Discover the repository. Try from project_path first, then current directory.
    let repo = match Repository::discover(project_path) {
        Ok(r) => r,
        Err(_) => Repository::discover(".").map_err(|e| {
            format!(
                "Failed to discover git repository. Ensure you are in a git repository and project paths are correct. Error: {}",
                e
            )
        })?,
    };

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
    let relative_project_path = canonical_project_path
        .strip_prefix(&canonical_repo_workdir)
        .map_err(|_| {
            format!(
                "Project path '{}' (resolved to '{}') is not inside the git repository workdir '{}' (resolved to '{}'). Ensure the project path is a subdirectory of the repository.",
                project_path.display(),
                canonical_project_path.display(),
                repo_workdir_raw.display(),
                canonical_repo_workdir.display()
            )
        })?;

    let mut status_opts = StatusOptions::new();
    status_opts.include_untracked(true);
    status_opts.recurse_untracked_dirs(true);

    // If relative_project_path is empty, it means project_path is the repo root.
    // In this case, we don't set a pathspec, so it checks all files in the repo.
    if !relative_project_path.as_os_str().is_empty() {
        status_opts.pathspec(relative_project_path);
    }

    let statuses = repo.statuses(Some(&mut status_opts))?;

    // If statuses is not empty, there are changes or untracked files matching the scope.
    Ok(!statuses.is_empty())
}

/// Processes a single project directory: verifies `pyproject.toml` existence, and calls `bump_version`.
/// Returns `Ok(true)` if version was bumped, `Ok(false)` if skipped,
/// or `Err` for critical errors.
fn process_project_directory(
    project_path: &Path,
    bump_level: u8,
    release: bool,
    git_aware: bool,
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
        match has_git_changes_in_path(project_path) {
            Ok(true) => {
                println!(
                    "{} {}: Git changes detected in {}. Proceeding.",
                    "✅".green(),
                    "Info".green(),
                    project_path.display().to_string().cyan()
                );
            }
            Ok(false) => {
                println!(
                    "{} {}: No git changes detected in {}. Skipping version bump (git-aware mode).",
                    "ℹ️".blue(),
                    "Skipping".blue(),
                    project_path.display().to_string().cyan()
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
    let mut processed_any_project_this_pattern = false;

    for entry in entries {
        let path = match entry {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "{} {}: Error accessing path from glob pattern '{}': {}. Skipping this item.",
                    "❌".red(),
                    "Error".red(),
                    project_glob_pattern.yellow(),
                    e.to_string().red()
                );
                continue; // Skip this problematic entry, continue with others in the pattern.
            }
        };

        if !path.is_dir() {
            continue; // Skip files, only process directories.
        }
        found_paths_for_pattern = true;

        match process_project_directory(&path, bump_level, release, git_aware) {
            Ok(true) => {
                processed_any_project_this_pattern = true;
            }
            Ok(false) => {
                // Project was skipped (e.g., no pyproject.toml or no git changes in git_aware mode), message already printed.
            }
            Err(e) => {
                // Critical error during processing of a project, message already printed. Propagate to stop all operations.
                return Err(e);
            }
        }
    }

    if !found_paths_for_pattern {
        eprintln!(
            "{} {}: No directories found matching glob pattern '{}'",
            "⚠️".yellow(),
            "Warning".yellow(),
            project_glob_pattern.yellow()
        );
    }

    Ok(processed_any_project_this_pattern)
}

/// Main function entry point
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut processed_any_project_overall = false;

    for project_glob_pattern in &args.projects {
        match process_glob_pattern(
            project_glob_pattern,
            args.bump_level,
            args.release,
            args.git_aware,
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
            error_message.push_str(" In git-aware mode, this can also occur if no targeted projects had relevant git changes.");
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
