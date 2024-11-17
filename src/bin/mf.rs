use clap::Parser;
use dialoguer::{Confirm, Select};
use fs_extra::dir::CopyOptions;
use humansize::{format_size, BaseUnit, FormatSizeOptions};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::io::{self};
use std::path::PathBuf;
use strsim::normalized_damerau_levenshtein;
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
    #[arg(long, short, default_value_t = 0.6)]
    threshold: f64,

}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // 获取文件夹 A 和 B 的子文件夹
    let dst_folders = get_subfolders(&args.dst);
    let src_folders = get_subfolders(&args.src);

    // 创建一个映射表，用于存储文件夹 B 中每个子文件夹的所有可能匹配
    let matches: HashMap<String, Vec<(String, usize)>> = src_folders
        .iter()
        .map(|src_folder| (src_folder.clone(), find_possible_matches(src_folder, &dst_folders, args.threshold)))
        .collect();


    let opt = CopyOptions::default()
        .skip_exist(true);


    matches.iter()
        .for_each(|(src_folder, match_as)|
            {
                if !match_as.is_empty() {
                    let options: Vec<String> = match_as.iter()
                        .map(|(target_dir, score)| format!("{}%|{}", score, target_dir))
                        .chain(std::iter::once("Skip".to_string())).collect();
                    let selection = Select::new()
                        .with_prompt(format!("Move {src_folder} to"))
                        .items(&options[..])
                        .interact()
                        .expect("Failed to select an option");

                    if selection < match_as.len() {
                        let (selected_match, _) = &match_as[selection];

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


/// move files
fn move_files(opt: &CopyOptions, dst_full_path: &String, to_move: &[PathBuf]) {
    println!();

    let mut cur_file = String::new();
    let mut done = 0u64;
    let size_opt = FormatSizeOptions::default()
        .decimal_zeroes(2)
        .decimal_places(2)
        .base_unit(BaseUnit::Byte);
    fs_extra::move_items_with_progress(to_move,
                                       dst_full_path, opt,
                                       |prog|
                                           {
                                               (cur_file != prog.file_name).then(|| {
                                                   print!("\r[{}/{}]Moving {} to {}", format_size(done, size_opt), format_size(prog.total_bytes, size_opt), prog.file_name, dst_full_path);
                                                   cur_file = prog.file_name.to_string();
                                                   done += prog.file_total_bytes;
                                               });
                                               fs_extra::dir::TransitProcessResult::ContinueOrAbort
                                           })
        .expect("Failed to merge folders");
}

fn extract_to_move(src_full_path: &String) -> Vec<PathBuf> {
    WalkDir::new(src_full_path)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .map(|en| en.into_path())
        .collect::<Vec<_>>()
}


/// clean empty folder
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


/// get all subfolders in a given folder
fn get_subfolders(folder: &str) -> Vec<String> {
    WalkDir::new(folder)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir() && entry.path().parent().unwrap().to_str().unwrap() == folder)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect()
}

trait RemoveBrackets {
    fn remove_s_brackets(&self) -> String;
    fn remove_m_brackets(&self) -> String;
    fn remove_l_brackets(&self) -> String;
}

impl RemoveBrackets for str {
    fn remove_s_brackets(&self) -> String {
        let re = Regex::new(r"\(\D*\d*\D+\d*\D*\)").unwrap();
        re.replace_all(self, "").into_owned()
    }

    fn remove_m_brackets(&self) -> String {
        let re = Regex::new(r"\[\D*\d*\D+\d*\D*]").unwrap();
        re.replace_all(self, "").into_owned()
    }

    fn remove_l_brackets(&self) -> String {
        let re = Regex::new(r"\{\D*\d*\D+\d*\D*}").unwrap();
        re.replace_all(self, "").into_owned()
    }
}

trait Uid {
    fn uid(&self) -> Option<usize>;

    fn eq_uid(&self, other: &Self) -> bool;
}
impl Uid for str {
    fn uid(&self) -> Option<usize> {
        let re = Regex::new(r"(\d{6,})").unwrap();
        if re.find(self).is_some() {
            Some(re.find(self).unwrap().as_str().parse::<usize>().unwrap())
        } else {
            None
        }
    }

    fn eq_uid(&self, other: &str) -> bool {
        if let (Some(self_uid_val), Some(other_uid_val)) = (self.uid(), other.uid()) {
            self_uid_val == other_uid_val
        } else {
            false
        }
    }
}

fn eval_similarity(src: &str, dst: &str) -> f64 {
    normalized_damerau_levenshtein(src, dst)
        .max(normalized_damerau_levenshtein(src.remove_m_brackets()
                                                .remove_l_brackets()
                                                .remove_s_brackets().as_str(), dst))
        .max(if src.eq_uid(dst) { 1. } else { 0. })
}

/// find possible matches for a given string in a list of strings
fn find_possible_matches(src: &str, matches: &[String], threshold: f64) -> Vec<(String, usize)> {
    let mut possible_matches: Vec<_> = matches
        .iter()
        .map(|dir| (dir.to_owned(), (eval_similarity(src, dir) * 100.) as usize))
        .collect();

    // 按 Levenshtein 距离从高到低排序
    possible_matches.sort_by_key(|(_, score)| -(*score as isize));


    let end = possible_matches.iter().position(|(_, score)| score < &((threshold * 100.) as usize)).unwrap_or(possible_matches.len());
    possible_matches.truncate(end);
    possible_matches
}

