use clap::Parser;
use dialoguer::{Confirm, Select};
use fs_extra::dir::CopyOptions;
use std::collections::HashMap;
use std::fs;
use std::io::{self};
use std::path::PathBuf;
use strsim::levenshtein;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(name = "folder_merge")]
#[command(author, version, about, long_about = "Merge folders with similar names")]
struct Args {
    /// the source folder
    src: String,

    /// the destination folder
    dst: String,

    /// always create new folder
    #[arg(long, short, default_value_t = false)]
    create: bool,

    /// the levenshtein threashhold
    #[arg(long, short, default_value_t = 1)]
    threshold: usize,

}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // 获取文件夹 A 和 B 的子文件夹
    let dst_folders = get_subfolders(&args.dst);
    let src_folders = get_subfolders(&args.src);

    // 创建一个映射表，用于存储文件夹 B 中每个子文件夹的所有可能匹配
    let matches: HashMap<String, Vec<String>> = src_folders
        .iter()
        .map(|src_folder| (src_folder.clone(), find_possible_matches(src_folder, &dst_folders, args.threshold)))
        .collect();


    let opt = CopyOptions::default()
        .skip_exist(true);


    matches.iter()
        .for_each(|(src_folder, match_as)|
            {
                if !match_as.is_empty() {
                    let options: Vec<&str> = match_as.iter().map(String::as_str).chain(std::iter::once("Skip")).collect();
                    let selection = Select::new()
                        .with_prompt(format!("Move {src_folder} to"))
                        .items(&options[..])
                        .interact()
                        .expect("Failed to select an option");

                    if selection < match_as.len() {
                        let selected_match = &match_as[selection];

                        let src_full_path = format!("{}/{}", args.src, src_folder);
                        let dst_full_path = format!("{}/{}", args.dst, selected_match);


                        let to_move = &extract_to_move(&src_full_path);
                        move_files(&opt, &dst_full_path, to_move);


                        clean(&src_full_path);
                    }
                } else if args.create || Confirm::new()
                    .with_prompt(format!("Did not find a match for folder [{}] in [{}] Create a new folder?", src_folder, args.dst))
                    .interact()
                    .expect("Failed to read input") {
                    let src_full_path = format!("{}/{}", args.src, src_folder);
                    let dst_full_path = format!("{}/{}", args.dst, src_folder);
                    fs::create_dir(&dst_full_path).expect("Failed to create folder");

                    let to_move = &extract_to_move(&src_full_path);
                    move_files(&opt, &dst_full_path, to_move);
                    clean(&src_full_path);
                }
            });

    Ok(())
}

fn move_files(opt: &CopyOptions, dst_full_path: &String, to_move: &[PathBuf]) {
    fs_extra::move_items_with_progress(to_move,
                                       dst_full_path, opt,
                                       |prog|
                                           {
                                               println!("Moving {} to {}", prog.file_name, dst_full_path);
                                               fs_extra::dir::TransitProcessResult::ContinueOrAbort
                                           })
        .expect("Failed to merge folders");
}

fn extract_to_move(src_full_path: &String) -> Vec<PathBuf> {
    WalkDir::new(src_full_path)
        .min_depth(1)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .map(|en| en.into_path())
        .collect::<Vec<_>>()
}

fn clean(src_full_path: &String) {
    WalkDir::new(src_full_path)
        .min_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .collect::<Vec<_>>()
        .is_empty()
        .then(|| {
            println!("Cleaning empty folder {src_full_path}");
            fs::remove_dir_all(src_full_path)
        });
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

fn find_possible_matches(src: &str, matches: &[String], threshold: usize) -> Vec<String> {
    matches
        .iter()
        .filter(|folder_a| levenshtein(src, folder_a) <= threshold)
        .cloned()
        .collect()
}

