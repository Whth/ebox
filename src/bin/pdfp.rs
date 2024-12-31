use clap::Parser;
use lopdf::{Document, ObjectId};
use rayon::prelude::*;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};


/// Extract images from PDF files
#[derive(Parser)]
#[command(author, version, )]
struct Cli {
    /// Input file path
    #[arg(value_name = "FILE")]
    input: PathBuf,

    /// Output directory path
    #[arg(short, long, default_value = "./pdf-pictures")]
    output_dir: PathBuf,

    #[arg(short, long, default_value = "png")]
    ext: String,
}

fn extract_images(input_path: &PathBuf, output_dir: &PathBuf, ext: &String) -> Result<(), Box<dyn std::error::Error>> {
    let doc = Document::load(input_path).expect("Failed to load PDF");
    fs::create_dir_all(output_dir).expect("Failed to create output directory");

    doc.page_iter()
        .par_bridge()
        .for_each(
            |id|
                {
                    save_page_images(input_path, ext, &doc, id);
                }
        );


    Ok(())
}

fn save_page_images(input_path: &Path, ext: &String, doc: &Document, id: ObjectId) {
    let mut ct = 0;
    doc.get_page_images(id)
        .expect("Failed to get page images")
        .iter()
        .for_each(
            |img|
                {
                    let output_path = input_path.join(format!("{}-{ct}.{ext}", id.0));
                    let file = File::create(&output_path).expect("Failed to create output file");
                    let mut writer = BufWriter::new(file);
                    writer.write_all(img.content).expect("Failed to write to output file");
                    ct += 1;
                }
        );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    extract_images(&cli.input, &cli.output_dir, &cli.ext).expect("Failed to extract images");
    Ok(())
}



