use clap::Parser;
use csv::{Reader, StringRecord, Writer};
use dialoguer::Select;
use std::error::Error;
use std::fs::File;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to the input CSV file.
    input: PathBuf,
    /// Path for the output CSV file.
    output: PathBuf,
    /// Group size for assigning item_ids. (default: 0)
    #[clap(short, long, default_value_t = 0)]
    group_size: usize,
}

fn select_timestamp_column(headers: &StringRecord) -> Result<usize, Box<dyn Error>> {
    Select::new()
        .with_prompt("Select the timestamp column")
        .items(&headers.iter().map(|h| h.to_string()).collect::<Vec<_>>())
        .default(0)
        .interact_opt()?
        .ok_or_else(|| "No column selected".into())
}

fn identify_empty_columns(headers: &StringRecord, records: &[StringRecord]) -> Vec<usize> {
    let mut columns_to_remove = Vec::new();
    let num_columns = headers.len();

    for col_idx in 0..num_columns {
        let mut is_all_empty = true;
        for record in records {
            if let Some(cell) = record.get(col_idx) {
                if !cell.trim().is_empty() {
                    is_all_empty = false;
                    break;
                }
            }
        }
        if is_all_empty {
            columns_to_remove.push(col_idx);
        }
    }
    columns_to_remove
}

fn build_new_headers(
    original_headers: &StringRecord,
    ts_idx: usize,
    columns_to_remove: &[usize],
) -> Vec<String> {
    let mut new_headers: Vec<String> = Vec::new();
    for (idx, header) in original_headers.iter().enumerate() {
        if columns_to_remove.contains(&idx) {
            continue;
        }
        let new_header = if idx == ts_idx {
            "timestamp".to_string()
        } else {
            header.to_string()
        };
        new_headers.push(new_header);
    }
    new_headers.push("item_id".to_string());
    new_headers
}

fn process_and_write_records(
    wtr: &mut Writer<File>,
    records: &[StringRecord],
    columns_to_remove: &[usize],
    group_size: usize,
) -> Result<usize, Box<dyn Error>> {
    let record_count = records.len();
    for (i, record) in records.iter().enumerate() {
        let mut new_record: Vec<String> = Vec::new();
        for (idx, field) in record.iter().enumerate() {
            if columns_to_remove.contains(&idx) {
                continue;
            }
            new_record.push(field.to_string());
        }

        let item_id = if group_size == 0 {
            1
        } else {
            (i / group_size) + 1
        };
        new_record.push(item_id.to_string());

        wtr.write_record(&new_record)?;
    }
    Ok(record_count)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let mut reader = Reader::from_path(&args.input)?;
    let mut wtr = Writer::from_path(&args.output)?;

    println!("Processing CSV with group_size: {}", args.group_size);

    let headers = reader.headers()?.clone();
    let ts_idx = select_timestamp_column(&headers)?;

    let records: Vec<StringRecord> = reader.into_records().collect::<Result<_, _>>()?;

    let columns_to_remove = identify_empty_columns(&headers, &records);

    let new_headers = build_new_headers(&headers, ts_idx, &columns_to_remove);
    wtr.write_record(&new_headers)?;

    let processed_record_count =
        process_and_write_records(&mut wtr, &records, &columns_to_remove, args.group_size)?;

    wtr.flush()?;
    println!("Total records processed: {}", processed_record_count);
    println!("CSV processing completed successfully.");

    Ok(())
}
