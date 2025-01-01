use clap::Parser;
use docx_rs::BreakType::TextWrapping;
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

#[derive(Default)]
struct DocBuilder {
    doc: Docx,
}


impl DocBuilder {
    fn add_style(mut self) -> Self {
        self.doc = self.doc.add_style(Style::new("Heading 1", StyleType::Paragraph)
            .name("Heading 1")
            .bold()
            .size(32)
            .fonts(RunFonts::new().east_asia("FangSong_GB2312")))
            .add_style(Style::new("Heading 2", StyleType::Paragraph)
                .name("Heading 2")
                .size(26)
                .fonts(RunFonts::new().east_asia("SimHei")))
            .add_style(Style::new("Heading 3", StyleType::Paragraph)
                .name("Heading 3")
                .size(22)
                .bold()
                .fonts(RunFonts::new().east_asia("KaiTi_GB2312")))
            .add_style(Style::new("Main", StyleType::Paragraph)
                .name("Main")
                .fonts(RunFonts::new().east_asia("DengXian"))
                .size(22)
                .indent(None, Some(SpecialIndentType::FirstLine(440)), None, None)
            );
        self
    }

    fn add_numbering(mut self) -> Self {
        self.doc = self.doc.add_abstract_numbering(
            AbstractNumbering::new(1)
                .add_level(
                    Level::new(
                        0,
                        Start::new(1),
                        NumberFormat::new("decimal"),
                        LevelText::new("%1"),
                        LevelJc::new("left"),
                    )
                        .suffix(LevelSuffixType::Space)
                        .indent(Some(0), Some(SpecialIndentType::Hanging(0)), Some(0), None)
                        .is_lgl()
                )
                .add_level(
                    Level::new(
                        1,
                        Start::new(1),
                        NumberFormat::new("decimal"),
                        LevelText::new("%1.%2"),
                        LevelJc::new("left"),
                    )
                        .suffix(LevelSuffixType::Space)
                        .indent(Some(0), Some(SpecialIndentType::Hanging(0)), Some(0), None)
                        .is_lgl(),
                )
                .add_level(
                    Level::new(
                        2,
                        Start::new(1),
                        NumberFormat::new("decimal"),
                        LevelText::new("%1.%2.%3"),
                        LevelJc::new("left"),
                    )
                        .suffix(LevelSuffixType::Space)
                        .indent(Some(0), Some(SpecialIndentType::Hanging(0)), Some(0), None)
                        .is_lgl()
                ),
        )
            .add_numbering(Numbering::new(1, 1));
        self
    }

    // 提供一个构建完成的方法以返回最终的Docx对象
    fn build(self) -> Docx {
        self.doc
    }
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


    // Create a new DOCX document and add abstract numbering definitions
    let mut doc = DocBuilder::default()
        .add_style()
        .add_numbering()
        .build();


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
                        .add_run(Run::new().add_text(&caps[1]))
                        .numbering(NumberingId::new(1), IndentLevel::new(0))
                        .style("Heading 1")
                }
                (_, Some(caps), _) => {
                    println!("  {}", &caps[0]);
                    Paragraph::new()
                        .add_run(Run::new().add_text(&caps[2]))
                        .numbering(NumberingId::new(1), IndentLevel::new(1))
                        .style("Heading 2")
                }
                (_, _, Some(caps)) => {
                    println!("    {}", &caps[0]);
                    Paragraph::new()
                        .add_run(Run::new().add_text(&caps[3]))
                        .numbering(NumberingId::new(1), IndentLevel::new(2))
                        .style("Heading 3")
                }
                _ => {
                    // Add non-title lines as regular paragraphs
                    Paragraph::new()
                        .add_run(Run::new().add_text(line).add_break(TextWrapping))
                        .style("Main")
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
