use clap::Parser;
use docx_rs::{Docx, Paragraph, Run};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

/// Convert text files to DOCX format
#[derive(Parser)]
#[command(author, version)]
struct Cli {
    /// Input file path (TXT)
    #[arg(value_name = "FILE")]
    input: PathBuf,

    /// Output file path (DOCX)
    #[arg(short, long, default_value = "./output.docx")]
    output: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Read the content of the input TXT file
    let mut txt_file = File::open(&cli.input)?;
    let mut txt_content = String::new();
    txt_file.read_to_string(&mut txt_content)?;

    // Create a new DOCX document and add the content as a paragraph

    Docx::default()
        .add_paragraph(Paragraph::new().add_run(Run::default().add_text(txt_content)))
        .build()
        .pack(File::create(&cli.output).expect("Failed to create output file"))
        .expect("Failed to write DOCX file");


    println!("Conversion successful! Output saved to {:?}", cli.output);
    Ok(())
}



