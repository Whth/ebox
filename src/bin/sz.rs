use clap::Parser;
use humansize::{format_size, BaseUnit, FormatSizeOptions};
use prettytable::{row, table};
use rayon::prelude::*;
use std::io::{self};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;
#[derive(Parser)]
#[command(name = "sz")]
#[command(author, version, about, long_about = "Get the size of subfolders")]
struct Args {
    /// the source folder
    #[arg(default_value = ".")]
    src: String,

    #[arg(short, long)]
    explore_greatest_dir: bool,

}

fn main() -> io::Result<()> {
    let arg = Args::parse();

    let path = PathBuf::from(arg.src);
    if path.is_dir() {
        let opt = FormatSizeOptions::default()
            .base_unit(BaseUnit::Byte);

        let mut seq: Vec<_> = WalkDir::new(path)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(Result::ok)
            .map(|p| (p.path().to_owned(), get_size(p.into_path())))
            .collect();
        seq.sort_by_key(|(_, sz)| -(*sz as i64));
        let mut disp_table = table!(["Folder", "Size"]);
        seq.iter().for_each(|(name, sz)| { disp_table.add_row(row![name.to_str().expect("Invalid UTF-8").to_string(), format_size(*sz,opt)]); });
        disp_table.printstd();
        if arg.explore_greatest_dir {
            Command::new("explorer")
                .arg(seq.iter().find(|(p, _)| p.is_dir()).expect("No directory found").0.to_str().expect("Invalid UTF-8"))
                .spawn()
                .expect("Failed to open directory")
                .wait()
                .expect("Failed to wait");
        }
    }

    Ok(())
}


fn get_size<P: AsRef<Path>>(path: P) -> u64 {
    WalkDir::new(path)
        .into_iter()
        .par_bridge()
        .filter_map(Result::ok)
        .filter(|p| p.file_type().is_file())
        .map(|p| p.metadata().expect("Failed to read metadata").len())
        .sum()
}