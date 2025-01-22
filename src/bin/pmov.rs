use clap::Parser;
use indicatif::ParallelProgressIterator;
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
#[derive(Debug)]
struct Bucket {
    min_size: usize,
    cur_size: usize,
    dir_paths: Vec<PathBuf>,
}

impl Bucket {
    fn new(min_size: usize) -> Self {
        Bucket {
            min_size,
            cur_size: 0,
            dir_paths: Vec::new(),
        }
    }

    fn add_path(&mut self, path: &Path) -> &Self {
        let num_files = fs::read_dir(path).expect("Failed to read directory").count();
        self.cur_size += num_files;
        self.dir_paths.push(path.to_path_buf());
        self
    }

    fn is_amplified(&self) -> bool {
        self.cur_size >= self.min_size
    }

    fn clear(&mut self) -> &Self {
        self.cur_size = 0;
        self.dir_paths.clear();
        self
    }

    fn dump_to(&mut self, path: &PathBuf, clear: bool) -> &Self {
        self.dir_paths
            .iter()
            .par_bridge()
            .progress_count(self.dir_paths.len() as u64)
            .for_each(
                |dir_path| {
                    fs_extra::move_items(&[dir_path], &path, &fs_extra::dir::CopyOptions::new()
                        .skip_exist(true))
                        .expect("Failed to copy directory");
                },
            );
        clear.then(|| self.clear());
        self
    }
}

#[derive(Debug)]
struct TargetGenerator {
    target_root: PathBuf,
}

impl TargetGenerator {
    fn new(target_root: PathBuf) -> Self {
        TargetGenerator { target_root }
    }

    fn gen_next_target(&self, create: bool) -> PathBuf {
        let max_index = self._find_max_index();
        let target_path = self.target_root.join(format!("{}th", max_index + 1));
        if create {
            fs::create_dir_all(&target_path).expect("Failed to create target directory");
        }
        target_path
    }

    fn _detect_existing_targets(&self) -> Vec<PathBuf> {
        fs::read_dir(&self.target_root)
            .expect("Failed to read target root")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir() && path.file_name().expect("Invalid path").to_string_lossy().ends_with("th"))
            .collect()
    }

    fn _find_max_index(&self) -> usize {
        let targets = self._detect_existing_targets();
        if !targets.is_empty() {
            targets.iter()
                .filter_map(|folder| folder.file_stem().and_then(|stem| stem.to_str()))
                .filter_map(|name| name.strip_suffix("th").and_then(|num| num.parse::<usize>().ok()))
                .max().unwrap_or(0)
        } else {
            0
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "pmov")]
///A simple program to move directories based on file count.
struct Cli {
    /// The source directory
    source: PathBuf,

    /// The target directory
    target: PathBuf,

    #[arg(short, long, default_value_t = 1000)]
    /// The minimum size of a bucket
    min_bucket_size: usize,

    #[arg(short, long, default_value_t = String::from("_"))]
    /// The separator between the directory name and the user id
    uid_separator: String,
}

fn main() {
    let cli = Cli::parse();

    let detected_source_dirs: Vec<_> = fs::read_dir(cli.source)
        .expect("Failed to read source directory")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.file_name().expect("Invalid path").to_string_lossy().contains(&cli.uid_separator))
        .collect();

    println!("Detected source dirs: {}", detected_source_dirs.len());

    let mut bucket = Bucket::new(cli.min_bucket_size);
    let target_gen = TargetGenerator::new(cli.target);

    for source_dir in detected_source_dirs {
        println!("Checking {:?}", source_dir);
        bucket.add_path(&source_dir);

        if bucket.is_amplified() {
            println!("Amplified with {} files", bucket.cur_size);
            let tar = target_gen.gen_next_target(true);
            println!("Moving to {:?}", tar);
            bucket.dump_to(&tar, true);
        }
    }

    if bucket.cur_size > 0 {
        println!("Handling remaining directories...");
        let tar = target_gen.gen_next_target(true);
        println!("Moving to {:?}", tar);
        bucket.dump_to(&tar, true);
    }
}



