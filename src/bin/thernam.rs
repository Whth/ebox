use clap::Parser;
use dialoguer::{theme::ColorfulTheme, Input, Select};
// Added Input
use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};
// Removed: use regex::Regex;

/// Custom error type for the application.
#[derive(Debug)]
enum AppError {
    /// IO error.
    Io(io::Error),
    // Removed: Regex(regex::Error),
    /// Error during user interaction (e.g., dialoguer).
    Interaction(dialoguer::Error),
    // Removed: StudentInfoExtraction(String),
    /// No files found in the specified directory.
    NoFilesFound(PathBuf),
    /// General processing error.
    Processing(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Io(err) => write!(f, "IO error: {}", err),
            // Removed: AppError::Regex(err) => write!(f, "Regex error: {}", err),
            AppError::Interaction(err) => write!(f, "Interaction error: {}", err),
            // Removed: AppError::StudentInfoExtraction(path_str)
            AppError::NoFilesFound(dir) => {
                write!(f, "No files found in directory: {}", dir.display())
            }
            AppError::Processing(msg) => write!(f, "Processing error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::Io(err) => Some(err),
            // Removed: AppError::Regex(err) => Some(err),
            AppError::Interaction(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> Self {
        AppError::Io(err)
    }
}

// Removed: impl From<regex::Error> for AppError

impl From<dialoguer::Error> for AppError {
    fn from(err: dialoguer::Error) -> Self {
        AppError::Interaction(err)
    }
}

/// Command-line arguments for the file renaming utility.
#[derive(Parser, Debug)]
#[command(author, version, about = "Renames student project files based on a predefined structure.", long_about = None)]
struct Args {
    /// Directory to process files from. Defaults to the current directory.
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// Output directory for renamed files. Defaults to the same as the input directory.
    #[arg(short, long)]
    output_dir: Option<PathBuf>,
    // Removed: student_info_regex field
}

/// Defines the standard document types and their processing order.
const DOC_TYPES: &[&str] = &[
    "任务书",
    "文献综述",
    "外文翻译",
    "开题报告",
    "skip", // Special type, can be used to skip a step or assign a generic file
    "教师中期检查表",
    "毕业设计过程稿",
    "毕业设计过程稿图纸",
    "毕业设计定稿",
    "毕业设计定稿图纸",
    "指导记录表",
    "指导教师评阅",
    "评阅教师评阅",
];

/// Main application logic.
fn main() -> Result<(), AppError> {
    let args = Args::parse();
    let input_dir = args.dir.unwrap_or_else(|| PathBuf::from("."));
    let output_dir = args.output_dir.unwrap_or_else(|| input_dir.clone());

    // Ensure output directory exists
    if !output_dir.exists() {
        fs::create_dir_all(&output_dir)?;
    }

    println!("Processing directory: {}", input_dir.display());
    println!("Output directory: {}", output_dir.display());

    let files = list_files_in_directory(&input_dir)?;
    if files.is_empty() {
        return Err(AppError::NoFilesFound(input_dir));
    }

    // Prepare a list of references to all files for selection prompts
    let all_file_entries_for_selection: Vec<&fs::DirEntry> = files.iter().collect();

    let theme = ColorfulTheme::default();

    // Prompt user for student ID
    let student_id: String = Input::with_theme(&theme)
        .with_prompt("Enter student ID")
        .interact_text()?;

    // Prompt user for student name
    let student_name: String = Input::with_theme(&theme)
        .with_prompt("Enter student name")
        .interact_text()?;

    println!(
        "Using student info: ID = {}, Name = {}",
        student_id, student_name
    );

    for (index, &doc_type) in DOC_TYPES.iter().enumerate() {
        println!("\nProcessing document type: {}", doc_type);

        // User selects from all files in the directory for the current document type
        let selected_entry = prompt_for_file_selection(&all_file_entries_for_selection, doc_type)?;
        let selected_path = selected_entry.path();

        let new_filename = construct_new_filename(
            index + 1,
            &student_id,
            &student_name,
            doc_type,
            &selected_path,
        )?;

        let destination_path = output_dir.join(&new_filename);

        copy_file_to_output(&selected_path, &destination_path)?;
        println!("Copied and renamed to: {}", destination_path.display());
    }

    println!("\nProcessing complete.");
    Ok(())
}

/// Lists all files in the specified directory.
///
/// # Arguments
/// * `dir` - The directory to scan.
///
/// # Returns
/// A `Result` containing a vector of `fs::DirEntry` or an `AppError`.
fn list_files_in_directory(dir: &Path) -> Result<Vec<fs::DirEntry>, AppError> {
    Ok(fs::read_dir(dir)?
        .filter_map(|entry_result| entry_result.ok())
        .filter(|entry| entry.file_type().map_or(false, |ft| ft.is_file()))
        .collect::<Vec<_>>())
}

// Removed: extract_student_info_from_filename function

/// Prompts the user to select a file from a list of candidates.
///
/// # Arguments
/// * `candidates` - A slice of `fs::DirEntry` references representing candidate files.
/// * `doc_type` - The document type for which the selection is being made (for the prompt message).
///
/// # Returns
/// A `Result` containing a reference to the selected `fs::DirEntry` or an `AppError`.
fn prompt_for_file_selection<'a>(
    candidates: &[&'a fs::DirEntry],
    doc_type: &str,
) -> Result<&'a fs::DirEntry, AppError> {
    let theme = ColorfulTheme::default();
    let items: Vec<String> = candidates
        .iter()
        .map(|e| {
            e.path()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();

    if items.is_empty() {
        // This case should ideally be prevented by the check in main() after list_files_in_directory
        return Err(AppError::Processing(format!(
            "No items to select for {}. This might indicate an empty input directory.",
            doc_type
        )));
    }

    let selection_index = Select::with_theme(&theme)
        .with_prompt(format!("Select file for \"{}\"", doc_type))
        .items(&items)
        .default(0)
        .interact()?;

    Ok(candidates[selection_index])
}

/// Constructs the new filename based on the predefined format.
/// Format: "{index}-{student_id}{student_name}[{doc_type}].{extension}"
///
/// # Arguments
/// * `index` - The 1-based index for the document type.
/// * `student_id` - The student's ID.
/// * `student_name` - The student's name.
/// * `doc_type` - The type of the document.
/// * `original_path` - The path of the original file, used to get the extension.
///
/// # Returns
/// A `Result` containing the new filename string or an `AppError`.
fn construct_new_filename(
    index: usize,
    student_id: &str,
    student_name: &str,
    doc_type: &str,
    original_path: &PathBuf,
) -> Result<String, AppError> {
    let prefix = format!("{}-{}", index, student_id);
    let target_name_tag = format!("[{}]", doc_type);
    let extension = original_path
        .extension()
        .unwrap_or_default()
        .to_string_lossy();

    if extension.is_empty() {
        Ok(format!("{}{}{}", prefix, student_name, target_name_tag))
    } else {
        Ok(format!(
            "{}{}{}.{}",
            prefix, student_name, target_name_tag, extension
        ))
    }
}

/// Copies a file to the specified destination path.
///
/// # Arguments
/// * `source_path` - The path of the file to copy.
/// * `destination_path` - The path where the file should be copied to.
///
/// # Returns
/// An `AppError` if the copy operation fails.
fn copy_file_to_output(source_path: &PathBuf, destination_path: &PathBuf) -> Result<(), AppError> {
    fs::copy(source_path, destination_path)?;
    Ok(())
}
