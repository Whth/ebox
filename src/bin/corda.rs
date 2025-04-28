use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    /// 输入文件路径（位置参数）
    #[arg(index = 1)]
    input: PathBuf,

    /// 输出文件路径（当未使用--inplace时必填）
    #[arg(short, long, required_unless_present = "inplace")]
    output: Option<PathBuf>,

    /// 原地排序（覆盖输入文件）
    #[arg(long)]
    inplace: bool,
}

struct CitationMatch {
    start: usize,
    end: usize,
    key: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

    // 处理输入输出路径
    let input_path = &args.input;
    let output_path = if args.inplace {
        input_path.clone()
    } else {
        args.output.expect("输出路径验证失败")
    };

    println!("--- 处理开始 ---");
    println!("输入文件: {}", input_path.display());
    println!("输出文件: {}", output_path.display());
    println!("原地模式: {}", if args.inplace { "启用" } else { "禁用" });

    // 读取文件内容
    let content = fs::read_to_string(input_path)?;
    println!("\n读取到 {} 字节的输入内容", content.len());

    // 正则匹配
    let re = Regex::new(r"#cite\(<([^>]+)>\)")?;
    let mut key_order = HashMap::new();
    let mut current_index = 0;
    let mut matches = Vec::new();

    // 收集引用并记录首次出现顺序
    for cap in re.captures_iter(&content) {
        let key = cap[1].to_string();
        key_order.entry(key.clone()).or_insert_with(|| {
            let idx = current_index;
            current_index += 1;
            idx
        });
        matches.push(CitationMatch {
            start: cap.get(0).unwrap().start(),
            end: cap.get(0).unwrap().end(),
            key,
        });
    }
    println!("\n发现 {} 个独特引用键", key_order.len());

    // 合并连续引用块
    let mut blocks = Vec::new();
    let mut current_block: Vec<CitationMatch> = Vec::new(); // 显式类型声明

    for m in matches {
        if current_block.is_empty() || m.start == current_block.last().unwrap().end {
            current_block.push(m);
        } else {
            blocks.push(current_block);
            current_block = vec![m];
        }
    }
    if !current_block.is_empty() {
        blocks.push(current_block);
    }
    println!("合并为 {} 个连续引用块", blocks.len());

    // 处理每个引用块
    let mut processed_blocks: Vec<(usize, usize, String)> = Vec::new(); // 显式类型声明
    for block in blocks {
        let original_keys: Vec<_> = block.iter().map(|m| &m.key).collect();
        let mut sorted_keys = original_keys.clone();
        sorted_keys.sort_by_cached_key(|k| *key_order.get(*k).unwrap());

        // 生成调试信息
        println!(
            "\n处理引用块 (位置 {}-{}):",
            block[0].start,
            block.last().unwrap().end
        );
        println!("原始顺序: {:?}", original_keys);
        println!("排序后顺序: {:?}", sorted_keys);

        let sorted_str = sorted_keys
            .iter()
            .map(|k| format!("#cite(<{}>)", k))
            .collect::<String>();

        processed_blocks.push((
            block[0].start,
            block.last().unwrap().end,
            sorted_str, // 确保是String类型
        ));
    }

    // 重组最终文本
    let mut result = String::new();
    let mut prev_end = 0;
    for (start, end, s) in processed_blocks {
        result.push_str(&content[prev_end..start]);
        result.push_str(&s); // &String 自动解引用为 &str
        prev_end = end;
    }
    result.push_str(&content[prev_end..]);

    // 写入输出文件
    fs::write(&output_path, result)?;
    println!("\n处理完成！输出文件已保存到：{}", output_path.display());
    println!("--- 处理结束 ---");

    Ok(())
}
