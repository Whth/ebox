use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

use semver::{BuildMetadata, Prerelease, Version};

/// Increment patch version and clear pre-release and build metadata
pub fn increment_patch(version: &mut Version) {
    version.patch += 1;
    version.pre = Prerelease::EMPTY;
    version.build = BuildMetadata::EMPTY;
}

/// Increment minor version, reset patch, and clear pre-release and build metadata
pub fn increment_minor(version: &mut Version) {
    version.minor += 1;
    version.patch = 0;
    version.pre = Prerelease::EMPTY;
    version.build = BuildMetadata::EMPTY;
}

/// Increment major version, reset minor/patch, and clear pre-release and build metadata
pub fn increment_major(version: &mut Version) {
    version.major += 1;
    version.minor = 0;
    version.patch = 0;
    version.pre = Prerelease::EMPTY;
    version.build = BuildMetadata::EMPTY;
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
    bump_version(pyproject_path.to_str().unwrap(), args.bump_level, args.release)?;
    Ok(())
}

/// Bumps the version of the specified project.
///
/// # Arguments
///
/// * `pyproject_path` - Path to the `pyproject.toml` file.
/// * `level` - Bump level (0~3), corresponding to dev/patch/minor/major.
/// * `release` - If true, makes the version a release version (removes pre-release/build).
pub fn bump_version(pyproject_path: &str, level: u8, release: bool) -> Result<(), Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(pyproject_path)?;
    let mut doc = contents.parse::<DocumentMut>()?;

    let version_str = get_version_from_toml(&doc)?;
    let mut version = Version::parse(version_str)?;

    match level {
        0 => {
            if release {
                // Make it a release version: remove pre-release and build metadata
                version.pre = Prerelease::EMPTY;
                version.build = BuildMetadata::EMPTY;
            } else {
                // Default behavior: bump dev version
                bump_dev(&mut version)?;
            }
        }
        1 => increment_patch(&mut version), // These already clear pre/build
        2 => increment_minor(&mut version), // These already clear pre/build
        3 => increment_major(&mut version), // These already clear pre/build
        _ => {
            eprintln!("Too many -i flags: use up to 3");
            std::process::exit(1);
        }
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
fn bump_dev(version: &mut Version) -> Result<(), Box<dyn std::error::Error>> {
    let pre = &mut version.pre;
    if pre.is_empty() {
        *pre = "dev0".parse()?;
    } else if let Some(n) = pre.as_str().strip_prefix("dev") {
        let num: u64 = n.parse()?;
        *pre = format!("dev{}", num + 1).parse()?;
    } else {
        // If it's some other pre-release, convert it to dev0
        // This might be controversial, but keeps it simple.
        // Alternatively, one could error out or preserve it.
        // For now, aligns with making it a 'dev' version.
        *pre = "dev0".parse()?;
        // Alternatively, to error out:
        // return Err("Unsupported pre-release format for dev bump, expected 'devN' or empty".into());
    }
    // Build metadata is not touched by bump_dev
    Ok(())
}
