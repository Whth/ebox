use clap::Parser;
use walkdir::WalkDir;
use std::collections::HashMap;
use std::fs;
use std::io::{self};
use std::path::PathBuf;
use strsim::levenshtein;
use dialoguer::{Input, Select};
use fs_extra::dir::CopyOptions;

#[derive(Parser)]
#[command(name = "folder_merge")]
#[command(author, version, about, long_about = "Merge folders with similar names")]
struct Args {
    /// the source folder
    src: String,

    /// the destination folder
    dst: String,
    
    
    /// always create new folder
    #[arg(long,short,default_value_t=false)]
    create: bool,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // 获取文件夹 A 和 B 的子文件夹
    let dst_folders = get_subfolders(&args.dst);
    let src_folders = get_subfolders(&args.src);

    // 创建一个映射表，用于存储文件夹 B 中每个子文件夹的所有可能匹配
    let matches: HashMap<String, Vec<String>> = src_folders
        .iter()
        .map(|src_folder| (src_folder.clone(), find_possible_matches(src_folder, &dst_folders)))
        .collect();

    
    matches.iter()
        .for_each(
        |(src_folder, match_as)|
            {
                if !match_as.is_empty() {
                    let options: Vec<&str> = match_as.iter().map(String::as_str).chain(std::iter::once("跳过")).collect();
                    let selection = Select::new()
                        .with_prompt("You can select one of the following options:")
                        .with_prompt(options
                            .iter().enumerate()
                            .map(|(i, s)| format!("{}. {}", i, s))
                            .collect::<Vec<String>>()
                            .join("\n").to_string())
                        .interact()
                        .expect("Failed to select an option");
                        
                    if selection < match_as.len() {
                        let selected_match = &match_as[selection];
                        merge_folders(&format!("{}/{}", args.src, src_folder), &format!("{}/{}", args.dst, selected_match)).expect("Failed to merge folders");
                    }
                } else if args.create||Input::<bool>::new()
                    .with_prompt(format!("Did not find a match for folder {} in folder B. Create a new folder?", src_folder))
                    .interact()
                    .expect("Failed to read input") {
                    let new_folder = format!("{}/{}", args.dst, src_folder);
                    fs::create_dir_all(&new_folder).expect("Failed to create new folder");
                    merge_folders(&format!("{}/{}", args.src, src_folder), &new_folder).expect("Failed to merge folders");
                }
            }
        );

    Ok(())
}

fn get_subfolders(folder: &str) -> Vec<String> {
    WalkDir::new(folder)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir() && entry.path().parent().unwrap().to_str().unwrap() == folder)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

fn find_possible_matches(folder_b: &str, folders_a: &[String]) -> Vec<String> {
    let threshold = 3; // 设置一个距离阈值，根据实际情况调整

    folders_a
        .iter()
        .filter(|folder_a| levenshtein(folder_b, folder_a) <= threshold)
        .cloned()
        .collect()
}

fn merge_folders(src: &str, dest: &str) -> io::Result<()> {
    WalkDir::new(src)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .try_for_each(|entry| {
            let src_path = entry.path();
            let relative_path = src_path.strip_prefix(src).unwrap();
            let dest_path = PathBuf::from(dest).join(relative_path);

            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            fs_extra::move_items(
                &[src_path],
                dest_path,
                &CopyOptions::new()
            ).expect("Failed to move file");
            Ok(())
        })
}