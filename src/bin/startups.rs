use clap::{Args, Parser, Subcommand};
use mslnk::ShellLink;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser)]
#[command(name = "StartupManager")]
#[command(version)]
#[command(about = "Manage startup applications on Windows", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add an application to startup
    Add(AddArgs),
    /// Remove an application from startup
    Remove(RemoveArgs),
    /// List all startup applications
    List,
    /// Open the startup directory in the file explorer
    View,
}

#[derive(Args)]
struct AddArgs {
    /// The path of the application to add to startup
    #[arg(required = true)]
    app_path: String,

    /// The name of the shortcut (without .lnk extension)
    #[arg(required = true)]
    name: String,

    /// The working directory for the application (defaults to the parent directory of the app)
    #[arg(short, long)]
    working_dir: Option<String>,
}

#[derive(Args)]
struct RemoveArgs {
    /// The name of the shortcut to remove (without .lnk extension)
    #[arg(required = true)]
    name: String,
}

fn get_startup_dir() -> PathBuf {
    let mut startup_dir = dirs::data_dir().expect("Unable to find home directory");
    startup_dir.push(r"Microsoft\Windows\Start Menu\Programs\Startup");
    startup_dir
}

fn capitalize_first_letter(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn convert_to_absolute(path: &str) -> PathBuf {
    let path_buf = Path::new(path);
    if path_buf.is_relative() {
        std::env::current_dir()
            .expect("Failed to get current directory")
            .join(path_buf)
    } else {
        path_buf.to_path_buf()
    }
}

fn main() {
    let cli = Cli::parse();
    let startup_dir = get_startup_dir();

    match &cli.command {
        Commands::Add(args) => {
            // Convert app_path to absolute path if it's relative
            let abs_app_path = convert_to_absolute(&args.app_path);

            // Let name's first letter be capitalized
            let name = capitalize_first_letter(&args.name);
            let lnk_path = startup_dir.join(format!("{}.lnk", name));

            let mut link = ShellLink::new(abs_app_path.to_str().unwrap()).unwrap_or_else(|e| {
                eprintln!("Failed to create shortcut: {}", e);
                std::process::exit(1);
            });

            // Set working directory
            let working_dir = if let Some(ref dir) = args.working_dir {
                convert_to_absolute(dir).to_string_lossy().to_string()
            } else {
                // Default to use the app's directory
                abs_app_path
                    .parent()
                    .expect("Failed to get parent directory")
                    .to_string_lossy()
                    .to_string()
            };

            link.set_working_dir(Some(working_dir));

            link.create_lnk(lnk_path.to_str().unwrap())
                .unwrap_or_else(|e| {
                    eprintln!("Failed to save shortcut: {}", e);
                    std::process::exit(1);
                });

            println!("Added {} as {}", abs_app_path.display(), lnk_path.display());
        }
        Commands::Remove(args) => {
            let lnk_path = startup_dir.join(format!("{}.lnk", args.name));
            if lnk_path.exists() {
                fs::remove_file(lnk_path).expect("Failed to remove the shortcut");
                println!("Removed shortcut for {}", args.name);
            } else {
                println!("Shortcut for {} does not exist.", args.name);
            }
        }
        Commands::List => {
            println!("{:<30}", "Shortcut Name");
            println!("{:-<30}", ""); // Separator line

            for entry in fs::read_dir(startup_dir).expect("Failed to read startup directory") {
                let entry = entry.expect("Failed to list contents");
                let path = entry.path();

                // Only handle .lnk files
                if path.extension().and_then(|ext| ext.to_str()) == Some("lnk") {
                    let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
                    println!("{:<30}", file_name);
                }
            }
        }
        Commands::View => {
            // Use system command to open startup directory
            Command::new("explorer")
                .arg(startup_dir.to_str().unwrap())
                .output()
                .expect("Failed to open startup directory");
            println!("Opened startup directory: {}", startup_dir.display());
        }
    }
}
