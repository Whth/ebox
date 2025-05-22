use clap::Parser;
use glob::glob;
use semver::{BuildMetadata, Prerelease, Version};
use std::fs;
use std::path::Path;
use std::process::Command;
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
    about = "A CLI tool to bump version in pyproject.toml for multiple projects, with glob support and optional git check.",
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

    /// Only bump version if git status is clean for the project directory
    #[arg(short = 'g', long, default_value_t = false)]
    git_check: bool,
}

/// Checks if the Git repository at the given path is dirty or not a valid git repo for checking.
/// Returns Ok(true) if dirty/unsuitable, Ok(false) if clean.
/// Returns Err if the git command itself fails for other reasons.
fn is_git_repo_dirty(project_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let output_result = Command::new("git")
        .current_dir(project_path)
        .arg("status")
        .arg("--porcelain")
        .output();

    match output_result {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.to_lowercase().contains("not a git repository") {
                    eprintln!(
                        "Warning: {} is not a git repository. Skipping version bump due to --git-check.",
                        project_path.display()
                    );
                    return Ok(true); // Treat as "cannot confirm clean" / "dirty" for decision logic
                }
                return Err(format!(
                    "git status command failed for '{}' with status {}: {}",
                    project_path.display(),
                    output.status,
                    stderr
                )
                .into());
            }
            if !output.stdout.is_empty() {
                eprintln!(
                    "Warning: {} has uncommitted changes. Skipping version bump due to --git-check.",
                    project_path.display()
                );
                return Ok(true); // Dirty
            }
            Ok(false) // Clean
        }
        Err(e) => Err(format!(
            "Failed to execute git command for '{}': {}",
            project_path.display(),
            e
        )
        .into()),
    }
}

/// Processes a single project directory: checks git status (if requested),
/// verifies `pyproject.toml` existence, and calls `bump_version`.
/// Returns `Ok(true)` if version was bumped, `Ok(false)` if skipped,
/// or `Err` for critical errors.
fn process_project_directory(
    project_path: &Path,
    bump_level: u8,
    release: bool,
    git_check: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    if git_check {
        match is_git_repo_dirty(project_path) {
            Ok(true) => {
                // Message already printed by is_git_repo_dirty or if it's just dirty
                return Ok(false); // Skipped due to git status
            }
            Ok(false) => {
                println!(
                    "Git status clean for {}. Proceeding.",
                    project_path.display()
                );
            }
            Err(e) => {
                return Err(e); // Propagate critical git errors
            }
        }
    }

    let pyproject_path = project_path.join("pyproject.toml");
    if !pyproject_path.exists() {
        eprintln!(
            "pyproject.toml not found in {}. Skipping.",
            project_path.display()
        );
        return Ok(false); // Skipped due to missing pyproject.toml
    }

    println!("Processing project: {}", project_path.display());
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
    git_check: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let entries = match glob(project_glob_pattern) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!(
                "Invalid glob pattern '{}': {}. Skipping this pattern.",
                project_glob_pattern, e
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
                    "Error accessing path from glob pattern '{}': {}. Skipping this item.",
                    project_glob_pattern, e
                );
                continue; // Skip this problematic entry, continue with others in the pattern.
            }
        };

        if !path.is_dir() {
            continue; // Skip files, only process directories.
        }
        found_paths_for_pattern = true;

        match process_project_directory(&path, bump_level, release, git_check) {
            Ok(true) => {
                processed_any_project_this_pattern = true;
            }
            Ok(false) => {
                // Project was skipped (e.g., dirty git, no pyproject.toml), continue to the next.
            }
            Err(e) => {
                // Critical error during processing of a project, propagate to stop all operations.
                return Err(e);
            }
        }
    }

    if !found_paths_for_pattern {
        eprintln!(
            "Warning: No directories found matching glob pattern '{}'",
            project_glob_pattern
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
            args.git_check,
        ) {
            Ok(processed_this_pattern) => {
                if processed_this_pattern {
                    processed_any_project_overall = true;
                }
            }
            Err(e) => {
                // A critical error occurred in process_glob_pattern (likely propagated from process_project_directory)
                // Stop all further processing and return the error.
                return Err(e);
            }
        }
    }

    if !processed_any_project_overall {
        return Err("No 'pyproject.toml' files were processed. Check paths and patterns.".into());
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

    // Step 1: Apply numeric increment if level > 0
    match level {
        1 => increment_patch(&mut version),
        2 => increment_minor(&mut version),
        3 => increment_major(&mut version),
        0 => {} // No numeric change based on level itself for this step
        _ => {
            // Changed from std::process::exit(1) to returning an error
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
    println!("Bumped version in {} to {}", pyproject_path, version);
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
        .ok_or_else(|| Box::<dyn std::error::Error>::from("'project' table not found in pyproject.toml"))?;

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
