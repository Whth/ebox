use clap::{Arg, ArgAction, Command};
use colored::Colorize;
use csv::ReaderBuilder;
use dialoguer::theme::ColorfulTheme;
use dialoguer::Input;
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
                    Some(self.records()
                        .filter_map(|r| r.ok())
                        .map(|r| r[pos].to_string())

                        .collect()
                    )
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
        let col_inds: Vec<usize> = column_names.iter().filter_map(|col_name| self.headers().unwrap().iter().position(|h| h == col_name)).collect();

        self.records().filter_map(|r| r.ok())
            .map(|r| col_inds.iter().map(|&i| r[i].to_string()).collect())
            .collect()
    }
}


fn extract_field(file_paths: &[PathBuf], extract_fields: Vec<String>) -> Vec<Vec<String>> {
    file_paths.iter().par_bridge()
        .filter(|&p| p.exists())
        .map(|path| File::open(path).expect("Failed to open file"))
        .map(|file| ReaderBuilder::new().has_headers(true).from_reader(BufReader::new(file)))
        .map(|mut r| {
            r.get_columns(extract_fields.clone())
        }).flatten().collect()
}


fn main() {
    // Set the number of threads to the number of CPU cores

    let app = make_app();

    let matches = app.get_matches();


    let file_paths: Vec<PathBuf> = matches.get_many::<String>("file_paths").unwrap().cloned().map(PathBuf::from).collect();
    let video_only = matches.get_flag("video_only");
    let audio_only = matches.get_flag("audio_only");
    let sub_only = matches.get_flag("sub_only");
    let cover_only = matches.get_flag("cover_only");
    let skip_sub = matches.get_flag("skip_sub");
    let skip_cover = matches.get_flag("skip_cover");
    let work_dir: String = matches.get_one::<String>("work_dir").unwrap().to_string();
    let interval: u64 = matches.get_one::<String>("interval").unwrap().parse::<u64>().unwrap();
    let url_tab = matches.get_one::<String>("url_tab").unwrap().to_owned();
    let title_tab = matches.get_one::<String>("title_tab").unwrap().to_owned();
    let clean_up = matches.get_flag("clean_failures");

    let data_rows = extract_field(&file_paths, vec![url_tab, title_tab]);
    // 使用标准库 io::stdin 和 io::stdout 实现 prompt 输入

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


    let options: Vec<&str> = vec![
        Some("--work-dir"),
        Some(work_dir.as_str()),
        video_only.then_some("--video-only"),
        audio_only.then_some("--audio-only"),
        sub_only.then_some("--sub-only"),
        cover_only.then_some("--cover-only"),
        skip_sub.then_some("--skip-subtitle"),
        skip_cover.then_some("--skip-cover"),
    ]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect();

    let mut rng = rand::thread_rng();


    let download_count = (start..end)
        .map(|ind| (ind, &data_rows[ind]))
        .map(|(ind, row)| (ind, &row[0], &row[1]))
        .filter_map(|(ind, url, title)| {
            println!("{}", format!("Checking [{}/{}]:{title} | {url}", ind + 1, data_rows.len()).cyan());
            if PathBuf::from(format!("{}/{}", work_dir, title)).exists()
                || PathBuf::from(format!("{}/{}.mp4", work_dir, title)).exists() {
                println!("{}", format!("File already exists, skipping: {title}").yellow());
                None
            } else { Some(url) }
        })
        .filter(|url| {
            let _success = StdProcessCommand::new("bbdown")
                .args(&options)
                .arg(url)
                .status()
                .expect("Failed to get exit status")
                .success();

            thread::sleep(Duration::from_secs(rng.gen_range(interval / 2..=interval / 3 * 2)));
            _success
        }).count();

    println!("{}", format!("Downloaded [{}/{}] files successfully.", download_count, data_rows.len()).blue());
    if clean_up {
        delete_numeric_dirs(&PathBuf::from(work_dir));
    }
}

fn delete_numeric_dirs(path: &PathBuf) {
    if path.is_dir() {
        fs::read_dir(path)
            .expect("Failed to read directory")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().expect("Failed to get file type").is_dir())
            .filter(|entry| {
                entry.file_name().to_str().expect("Invalid UTF-8").chars().all(|c| c.is_ascii_digit())
            })
            .try_for_each(|entry| {
                let path = entry.path();
                println!("Deleting directory: {:?}", path);
                fs::remove_dir_all(&path)
            })
            .expect("Failed to delete numeric directories");
    }
}
fn make_app() -> Command {
    Command::new("avd")
        .version("0.1.0")
        .about("Download videos from URLs listed in CSV files using BBDown.")
        .arg(Arg::new("file_paths")
            .value_name("CSV")
            .required(true)
            .help("One or more CSV files containing URLs to download"))
        .arg(Arg::new("video_only")
            .short('v')
            .long("video-only")
            .action(ArgAction::SetTrue)
            .help("Download video only."))
        .arg(Arg::new("audio_only")
            .short('a')
            .long("audio-only")
            .action(ArgAction::SetTrue)
            .help("Download audio only."))
        .arg(Arg::new("sub_only")
            .long("sub-only")
            .short('b')
            .action(ArgAction::SetTrue)
            .help("Download subtitles only."))
        .arg(Arg::new("cover_only")
            .long("cover-only")
            .short('c')
            .action(ArgAction::SetTrue)
            .help("Download cover image only."))
        .arg(Arg::new("skip_sub")
            .long("skip-sub")
            .short('s')
            .action(ArgAction::SetTrue)
            .help("Skip downloading subtitles."))
        .arg(Arg::new("skip_cover")
            .long("skip-cover")
            .short('d')
            .action(ArgAction::SetTrue)
            .help("Skip downloading cover image."))
        .arg(Arg::new("work_dir")
            .short('w')
            .long("work-dir")
            .value_name("DIR")
            .default_value(".")
            .help("Working directory for downloads."))
        .arg(Arg::new("interval")
            .short('i')
            .long("interval")
            .value_name("SECONDS")
            .default_value("5")
            .help("Interval between downloads in seconds.")
        )
        .arg(Arg::new("url_tab")
            .long("url-tab")
            .value_name("TAB")
            .default_value("url")
            .help("The tab name in the CSV file that contains the URLs. Default is 'url'.")
        )
        .arg(Arg::new("title_tab")
            .long("title-tab")
            .value_name("TITLE")
            .default_value("title")
            .help("The title of the video. If not provided, the title will be generated based on the URL.")
        )
        .arg(Arg::new("clean_failures")
            .long("clean-failures")
            .short('u')
            .action(ArgAction::SetTrue)
            .help("Clean up failed downloads."))
}