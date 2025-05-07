use clap::Parser;
use dialoguer::Select;
use std::error::Error;
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
    /// Delete empty rows (default: true)
    #[clap(short = 'e', long, default_value_t = false)]
    remain_empty: bool,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let mut reader = csv::Reader::from_path(&args.input)?;
    let mut wtr = csv::Writer::from_path(&args.output)?;

    println!("Processing CSV with group_size: {}", args.group_size);

    let headers = reader.headers()?.clone();
    // Replaced input column selection with select UI
    let ts_idx = Select::new()
        .with_prompt("Select the timestamp column")
        .items(&headers.iter().map(|h| h.to_string()).collect::<Vec<_>>())
        .default(0)
        .interact_opt()?
        .ok_or_else(|| "No column selected".to_string())?;

    let mut new_headers: Vec<String> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            if i == ts_idx {
                "timestamp".to_string()
            } else {
                h.to_string()
            }
        })
        .collect();

    new_headers.push("item_id".to_string());
    wtr.write_record(&new_headers)?;

    println!("Selected timestamp column: {}", headers[ts_idx].to_string());
    println!("Writing output to: {}", args.output.display());

    let mut record_count = 0;
    for (i, result) in reader.records().enumerate() {
        let record = result?;

        // Skip empty rows if delete_empty is true
        if args.remain_empty && record.iter().any(|field| field.is_empty()) {
            continue;
        }

        record_count += 1;

        let item_id = if args.group_size == 0 {
            1
        } else {
            (i / args.group_size) + 1
        };

        let mut new_record: Vec<String> = record
            .iter()
            .enumerate()
            .map(|(_, s)| s.to_string())
            .collect();

        new_record.push(item_id.to_string());
        wtr.write_record(&new_record)?;
    }

    wtr.flush()?;
    println!("Total records processed: {}", record_count);
    println!("CSV processing completed successfully.");

    Ok(())
}
