use clap::Parser;
use docx_rs::*;
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};
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
    let txt_file = File::open(&cli.input).expect("Failed to open input file");
    let reader = BufReader::new(txt_file);

    // Define regular expressions for different levels
    let re_level_1 = Regex::new(r"^\d+\.?\s+(.+)$")?;
    let re_level_2 = Regex::new(r"^\d+(\.\d+)\.?\s+(.+)$")?;
    let re_level_3 = Regex::new(r"^\d+(\.\d+){2}\.?\s+(.+)$")?;

    let header_1_font = RunFonts::new().east_asia("FangSong_GB2312");
    let header_2_font = RunFonts::new().east_asia("SimHei");

    // Create a new DOCX document and add abstract numbering definitions
    let mut doc = Docx::default()
        .add_abstract_numbering(
            AbstractNumbering::new(1).add_level(
                Level::new(
                    0,
                    Start::new(1),
                    NumberFormat::new("decimal"),
                    LevelText::new("%1."),
                    LevelJc::new("left"),
                )
                .bold()
                .size(32)
                .fonts(header_1_font.clone()),
            ),
        )
        .add_numbering(Numbering::new(1, 1))
        .add_abstract_numbering(
            AbstractNumbering::new(2).add_level(
                Level::new(
                    0,
                    Start::new(1),
                    NumberFormat::new("decimal"),
                    LevelText::new("%1.%2."),
                    LevelJc::new("left"),
                )
                .size(28)
                .fonts(header_2_font.clone()),
            ),
        )
        .add_numbering(Numbering::new(2, 2))
        .add_abstract_numbering(
            AbstractNumbering::new(3).add_level(
                Level::new(
                    0,
                    Start::new(1),
                    NumberFormat::new("decimal"),
                    LevelText::new("%1.%2.%3."),
                    LevelJc::new("left"),
                )
                .indent(Some(0), Some(SpecialIndentType::FirstLine(0)), None, None),
            ),
        )
        .add_numbering(Numbering::new(3, 3));

    // Process each line in the input file
    let paragraphs: Vec<Paragraph> = reader
        .lines()
        .map_while(Result::ok)
        .filter(|line| !line.is_empty())
        .map(|line| {
            match (
                re_level_1.captures(&line),
                re_level_2.captures(&line),
                re_level_3.captures(&line),
            ) {
                (Some(caps), _, _) => {
                    println!("{}", &caps[0]);
                    Paragraph::new()
                        .add_run(Run::new().add_text(&caps[1]).fonts(header_1_font.clone()))
                        .numbering(NumberingId::new(1), IndentLevel::new(0))
                }
                (_, Some(caps), _) => {
                    println!("  {}", &caps[0]);
                    Paragraph::new()
                        .add_run(Run::new().add_text(&caps[2]).fonts(header_2_font.clone()))
                        .numbering(NumberingId::new(2), IndentLevel::new(0))
                }
                (_, _, Some(caps)) => {
                    println!("    {}", &caps[0]);
                    Paragraph::new()
                        .add_run(Run::new().add_text(&caps[3]))
                        .numbering(NumberingId::new(3), IndentLevel::new(0))
                }
                _ => {
                    // Add non-title lines as regular paragraphs
                    Paragraph::new().add_run(Run::new().add_text(line))
                }
            }
        })
        .collect();

    for paragraph in paragraphs {
        doc = doc.add_paragraph(paragraph);
    }

    doc.build()
        .pack(File::create(&cli.output).expect("Failed to create output file"))
        .expect("Failed to save DOCX file");

    println!("Conversion successful! Output saved to {:?}", cli.output);
    Ok(())
}
