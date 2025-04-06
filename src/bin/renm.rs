use clap::Parser;
use std::collections::HashMap;
use std::fs;

/// A Tool that renames files in a directory and return a mapping file.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The directory to rename files in
    directory: String,

    /// The output mapping file name
    #[arg(short, long, default_value = "rename_map.json")]
    output: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let entries = fs::read_dir(&args.directory)?;
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .collect();

    files.sort_by(|a, b| a.file_name().to_str().cmp(&b.file_name().to_str()));

    let mut rename_ops = Vec::new();
    for (i, entry) in files.iter().enumerate() {
        let old_name = entry.file_name().to_str().unwrap().to_string();

        // 修正后的extension获取方式
        let extension = entry
            .path()
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_owned())
            .unwrap_or_default();

        let new_name = if extension.is_empty() {
            format!("{}", i + 1)
        } else {
            format!("{}.{}", i + 1, extension)
        };

        let new_path = entry.path().with_file_name(&new_name);
        rename_ops.push((entry.path().to_path_buf(), new_path, old_name));
    }

    for (_, new_path, _) in &rename_ops {
        if new_path.exists() {
            eprintln!("File already exists: {}", new_path.display());
            std::process::exit(1);
        }
    }

    for (old_path, new_path, _) in &rename_ops {
        fs::rename(old_path, new_path)?;
    }

    let mapping: HashMap<String, String> = rename_ops
        .into_iter()
        .enumerate()
        .map(|(i, (_, _, old_name))| (format!("{}", i + 1), old_name))
        .collect();

    let json = serde_json::to_string_pretty(&mapping)?;
    fs::write(&args.output, json)?;

    println!("Saved mapping to {}", args.output);
    Ok(())
}
