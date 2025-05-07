use clap::Parser;
use csv::ReaderBuilder;
use dialoguer::{theme::ColorfulTheme, MultiSelect, Select};
use std::error::Error;
use std::fs::File;
use std::path::PathBuf;
// Import PathBuf

/// adconv: A CLI tool to convert CSV files for anomaly detection.
///
/// This tool reads a CSV file, allows the user to select timestamp and target columns,
/// and transforms the data into a format suitable for anomaly detection tasks.
/// It supports two modes:
/// 1. Multiple item_ids: Each selected column (excluding the timestamp) becomes a separate item.
/// 2. Single item_id: A single selected column (excluding the timestamp) is used as the target,
///    and all records are assigned a default item_id of "0".
///
/// The output CSV will have three columns: "item_id", "timestamp", and "target".
#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the input CSV file.
    input: PathBuf,
    /// Path for the output CSV file.
    output: PathBuf,
    /// Group size for assigning item_ids. (default: 1)
    #[clap(short, long, default_value_t = 1)]
    group_size: usize,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // 读取 CSV 文件并获取表头
    let (headers, rdr) = read_csv_and_get_headers(&args.input)?;

    // 选择时间戳列
    let timestamp_col = select_timestamp_column(&headers)?;

    // 选择目标模式
    let (selected_cols, target_col) = select_target_mode(&headers, timestamp_col)?;

    // 创建输出文件
    let mut wtr = csv::Writer::from_path(&args.output)?;

    // 写入头
    wtr.write_record(&["item_id", "timestamp", "target"])?;

    // 处理每一行记录
    let record_count = process_records(
        rdr,
        &headers,
        timestamp_col,
        &selected_cols,
        target_col,
        &mut wtr,
        args.group_size, // 新增：传递 group_size 参数
    )?;

    println!(
        "Processed {} records into {}",
        record_count,
        args.output.display()
    );

    Ok(())
}

/// 读取 CSV 文件并获取表头
fn read_csv_and_get_headers(
    input: &PathBuf,
) -> Result<(Vec<String>, csv::Reader<File>), Box<dyn Error>> {
    let file = File::open(input)?;
    let mut rdr = ReaderBuilder::new().from_reader(file);
    let headers = rdr.headers()?.clone();
    Ok((headers.iter().map(|s| s.to_string()).collect(), rdr))
}

/// 选择时间戳列
fn select_timestamp_column(headers: &[String]) -> Result<usize, Box<dyn Error>> {
    let headers_str: Vec<&str> = headers.iter().map(|s| s.as_str()).collect();
    let default_idx = headers_str
        .iter()
        .position(|h| h.eq_ignore_ascii_case("timestamp"));

    if let Some(idx) = default_idx {
        println!(
            "Automatically selected '{}' as the timestamp column.",
            headers_str[idx]
        );
        Ok(idx)
    } else {
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Select the timestamp column")
            .items(&headers_str)
            .default(0)
            .interact()?;
        Ok(selection)
    }
}

/// 获取可用的列名和列索引（排除时间戳列）
fn get_available_columns(headers: &[String], timestamp_col: usize) -> (Vec<&str>, Vec<usize>) {
    let available_cols: Vec<&str> = headers
        .iter()
        .enumerate()
        .filter_map(|(i, h)| {
            if i != timestamp_col {
                Some(h.as_str())
            } else {
                None
            }
        })
        .collect();
    let col_indices: Vec<usize> = (0..headers.len()).filter(|&i| i != timestamp_col).collect();
    (available_cols, col_indices)
}

/// 选择目标模式
fn select_target_mode(
    headers: &[String],
    timestamp_col: usize,
) -> Result<(Option<Vec<usize>>, Option<usize>), Box<dyn Error>> {
    let target_mode = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select target mode")
        .item("Multiple item_ids (each selected column becomes an item)")
        .item("Single item_id (select one column as target)")
        .default(0)
        .interact()?;

    match target_mode {
        0 => {
            // 多列模式：选择多个列作为 item_id
            let (available_cols, col_indices) = get_available_columns(headers, timestamp_col);

            if available_cols.is_empty() {
                return Err("No other columns available besides timestamp".into());
            }

            let selected = MultiSelect::with_theme(&ColorfulTheme::default())
                .with_prompt("Select columns to convert into targets (each will become an item_id)")
                .items(&available_cols)
                .interact()?;

            let selected_indices: Vec<usize> =
                selected.into_iter().map(|i| col_indices[i]).collect();
            Ok((Some(selected_indices), None))
        }
        1 => {
            // 单列模式：选择一个目标列
            let (available_cols, col_indices) = get_available_columns(headers, timestamp_col);

            if available_cols.is_empty() {
                return Err(
                    "No other columns available besides timestamp to be the target column".into(),
                );
            }

            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Select the target column")
                .items(&available_cols)
                .default(0)
                .interact()?;

            let selected_col_idx = col_indices[selection];
            Ok((None, Some(selected_col_idx)))
        }
        _ => unreachable!(),
    }
}

/// 处理每一行记录
fn process_records(
    mut rdr: csv::Reader<File>,
    headers: &[String],
    timestamp_col: usize,
    selected_cols: &Option<Vec<usize>>,
    target_col: Option<usize>,
    wtr: &mut csv::Writer<File>,
    group_size: usize, // 新增：接收 group_size 参数
) -> Result<usize, Box<dyn Error>> {
    let mut record_count = 0;
    let mut group_id = 0; // 新增：用于跟踪当前组的 ID
    for result in rdr.records() {
        let record = result?;
        let timestamp = record.get(timestamp_col).unwrap_or_default();

        if let Some(cols) = selected_cols {
            // 多 item_id 模式
            for &col_idx in cols.iter() {
                let item_id_name = headers.get(col_idx).expect("Invalid column index");
                let value = record.get(col_idx).unwrap_or_default();
                wtr.write_record(&[item_id_name.as_str(), timestamp, value])?;
            }
        } else if let Some(target_col_idx) = target_col {
            // 单 item_id 模式
            let value = record.get(target_col_idx).unwrap_or_default();
            // 新增：根据 group_size 计算 item_id
            let item_id = format!("{}", group_id / group_size);
            wtr.write_record(&[&item_id, timestamp, value])?;
            group_id += 1; // 新增：更新组 ID
        }

        record_count += 1;
    }
    Ok(record_count)
}
