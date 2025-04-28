use clap::Parser;
use regex::Regex;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    /// 输入文件路径
    input_file: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    let content = fs::read_to_string(&args.input_file)?;

    // 正则表达式匹配 #cite(...) 模式
    let re = Regex::new(r##"#cite\([^)]+\)"##)?;
    let processed = re.replace_all(&content, |caps: &regex::Captures| {
        let match_start = caps.get(0).unwrap().start();

        // 检查前一个字符是否为 '等'
        let is_preceded_by_deng = if match_start > 0 {
            content[..match_start]
                .chars()
                .rev()
                .next()
                .map_or(false, |c| c == '等')
        } else {
            false
        };

        // 如果前一个字符是 '等' 则保留，否则删除
        if is_preceded_by_deng {
            caps[0].to_string()
        } else {
            String::new()
        }
    });

    // 覆盖写入原文件
    fs::write(&args.input_file, processed.as_bytes())?;
    Ok(())
}
