use clap::{ArgAction, Parser};
use colored::Colorize;
use csv::ReaderBuilder;
use dialoguer::theme::ColorfulTheme;
use dialoguer::Input;
use md5;
use rand::Rng;
use rayon::prelude::*;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::Command as StdProcessCommand;
use std::time::Duration;
use std::{fs, io, thread, vec};

trait GetColumn {
    #[allow(unused)]
    fn get_column(&mut self, column_name: &str) -> Option<Vec<String>>;

    fn get_columns(&mut self, column_names: Vec<String>) -> Vec<Vec<String>>;
}

impl<T> GetColumn for csv::Reader<T>
where
    T: io::Read,
{
    fn get_column(&mut self, column_name: &str) -> Option<Vec<String>> {
        match self.headers() {
            Ok(headers) => {
                if let Some(pos) = headers.iter().position(|s| s == column_name) {
                    Some(self.records().filter_map(|r| r.ok()).map(|r| r[pos].to_string()).collect())
                } else {
                    println!("Column '{column_name}' not found in the CSV file headers.");
                    None
                }
            }
            Err(e) => {
                println!("Error reading headers: {}", e);
                None
            }
        }
    }

    fn get_columns(&mut self, column_names: Vec<String>) -> Vec<Vec<String>> {
        let col_inds: Vec<usize> = column_names
            .iter()
            .filter_map(|col_name| self.headers().unwrap().iter().position(|h| h == col_name))
            .collect();
        self.records()
            .filter_map(|r| r.ok())
            .map(|r| col_inds.iter().map(|&i| r[i].to_string()).collect())
            .collect()
    }
}

fn extract_field(file_paths: &[PathBuf], extract_fields: Vec<String>) -> Vec<Vec<String>> {
    file_paths
        .iter()
        .par_bridge()
        .filter(|&p| p.exists())
        .map(|path| File::open(path).expect("Failed to open file"))
        .map(|file| ReaderBuilder::new().has_headers(true).from_reader(BufReader::new(file)))
        .map(|mut r| r.get_columns(extract_fields.clone()))
        .flatten()
        .collect()
}

fn url_hash(url: &str) -> String {
    format!("{}", String::from_utf8_lossy(&md5::compute(url.as_bytes()).0))
}

#[derive(Parser)]
#[clap(name = "avd", version = "0.1.0", about = "Download videos from URLs listed in CSV files using BBDown.")]
struct Cli {
    #[clap(value_name = "CSV", required = true, help = "One or more CSV files containing URLs to download")]
    file_paths: Vec<String>,

    #[clap(short = 'v', long = "video-only", action = ArgAction::SetTrue, help = "Download video only.")]
    video_only: bool,

    #[clap(short = 'a', long = "audio-only", action = ArgAction::SetTrue, help = "Download audio only.")]
    audio_only: bool,

    #[clap(short = 'b', long = "sub-only", action = ArgAction::SetTrue, help = "Download subtitles only.")]
    sub_only: bool,

    #[clap(short = 'c', long = "cover-only", action = ArgAction::SetTrue, help = "Download cover image only.")]
    cover_only: bool,

    #[clap(short = 's', long = "skip-sub", action = ArgAction::SetTrue, help = "Skip downloading subtitles.")]
    skip_sub: bool,

    #[clap(short = 'd', long = "skip-cover", action = ArgAction::SetTrue, help = "Skip downloading cover image.")]
    skip_cover: bool,

    #[clap(short = 'w', long = "work-dir", value_name = "DIR", default_value = ".", help = "Working directory for downloads.")]
    work_dir: String,

    #[clap(short = 'i', long = "interval", value_name = "SECONDS", default_value = "5", help = "Interval between downloads in seconds.")]
    interval: u64,

    #[clap(long = "url-tab", value_name = "TAB", default_value = "url", help = "The tab name in the CSV file that contains the URLs. Default is 'url'.")]
    url_tab: String,

    #[clap(long = "title-tab", value_name = "TITLE", default_value = "title", help = "The title of the video. If not provided, the title will be generated based on the URL.")]
    title_tab: String,

    #[clap(short = 'u', long = "clean-failures", action = ArgAction::SetTrue, help = "Clean up failed downloads.")]
    clean_failures: bool,
}

fn main() {
    let cli = Cli::parse();

    let file_paths: Vec<PathBuf> = cli.file_paths.iter().map(PathBuf::from).collect();

    let data_rows = extract_field(&file_paths, vec![cli.url_tab.clone(), cli.title_tab.clone()]);

    println!("Total URLs: {}", data_rows.len());

    let start: usize = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter the starting index")
        .default(0)
        .interact()
        .expect("Failed to read input");

    let end: usize = Input::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter the ending index")
        .default(data_rows.len())
        .interact()
        .expect("Failed to read input")
        .min(data_rows.len());

    let mut rng = rand::rng();

    let download_count = (start..end)
        .map(|ind| (ind, &data_rows[ind]))
        .map(|(ind, row)| (ind, &row[0], &row[1]))
        .filter_map(|(ind, url, title)| {
            let hash = url_hash(url);
            let hashed_title = format!("{}_{}", title, hash);
            println!(
                "{}",
                format!("Checking [{}/{}]:{title} | {url}", ind + 1, data_rows.len()).cyan()
            );
            if PathBuf::from(format!("{}/{}", cli.work_dir, hashed_title)).exists()
                || PathBuf::from(format!("{}/{}.mp4", cli.work_dir, hashed_title)).exists()
            {
                println!("{}", format!("File already exists, skipping: {title}").yellow());
                None
            } else {
                Some((url, hash))
            }
        })
        .filter_map(|(url, hash)| {

            let pat =format!("<videoTitle>_{}", hash);

            let options: Vec<&str> = vec![
                Some("--work-dir"),
                Some(cli.work_dir.as_str()),
                Some("--file-pattern"),
                Some(pat.as_str()),
                cli.video_only.then_some("--video-only"),
                cli.audio_only.then_some("--audio-only"),
                cli.sub_only.then_some("--sub-only"),
                cli.cover_only.then_some("--cover-only"),
                cli.skip_sub.then_some("--skip-subtitle"),
                cli.skip_cover.then_some("--skip-cover"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

            let _success = StdProcessCommand::new("bbdown")
                .args(&options)
                .arg(url)
                .status()
                .expect("Failed to get exit status")
                .success();
            thread::sleep(Duration::from_secs(rng.random_range(cli.interval / 2..=cli.interval / 3 * 2)));
            if _success { Some(url) } else { None }
        })
        .count();

    println!(
        "{}",
        format!(
            "Downloaded [{}/{}] files successfully.",
            download_count,
            data_rows.len()
        )
        .blue()
    );
    if cli.clean_failures {
        delete_numeric_dirs(&PathBuf::from(&cli.work_dir));
    }
}

fn delete_numeric_dirs(path: &PathBuf) {
    if path.is_dir() {
        fs::read_dir(path)
            .expect("Failed to read directory")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().expect("Failed to get file type").is_dir())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .expect("Invalid UTF-8")
                    .chars()
                    .all(|c| c.is_ascii_digit())
            })
            .try_for_each(|entry| {
                let path = entry.path();
                println!("Deleting directory: {:?}", path);
                fs::remove_dir_all(&path)
            })
            .expect("Failed to delete numeric directories");
    }
}