use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

use semver::{BuildMetadata, Prerelease, Version};

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
    about = "A minimal CLI tool to bump version in pyproject.toml",
    long_about = None
)]
struct Args {
    /// Path to the project directory, must contain pyproject.toml
    #[arg(default_value = ".")]
    project: PathBuf,

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
}

/// Main function entry point
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Build the path to pyproject.toml
    let pyproject_path = Path::new(&args.project).join("pyproject.toml");

    if !pyproject_path.exists() {
        eprintln!("pyproject.toml not found in {}", args.project.display());
        std::process::exit(1);
    }

    // Perform version bump
    bump_version(
        pyproject_path.to_str().unwrap(),
        args.bump_level,
        args.release,
    )?;
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
            eprintln!("Too many -i flags: use up to 3");
            std::process::exit(1); // Or return Err for main to handle
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
    println!("Bumped version to {}", version);
    Ok(())
}

/// Extracts the version string from the TOML document.
fn get_version_from_toml(doc: &DocumentMut) -> Result<&str, &'static str> {
    doc.get("project")
        .and_then(|p| p.get("version"))
        .and_then(Item::as_str)
        .ok_or("Version not found or invalid")
}

/// Updates the version field in the TOML document.
fn update_version_in_toml(doc: &mut DocumentMut, new_version: String) -> Result<(), &'static str> {
    doc["project"]["version"] = toml_edit::value(new_version);
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
