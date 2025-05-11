use chrono::{Duration, NaiveDate, NaiveDateTime};
use csv::Writer;
use rayon::prelude::ParallelSliceMut;
use std::error::Error;
use std::fs::File;
use std::path::PathBuf;

const BASE_DATETIME_NAIVE: NaiveDateTime = NaiveDate::from_ymd_opt(1900, 1, 1)
    .expect("Invalid base year, month, or day for NaiveDate")
    .and_hms_opt(0, 0, 0)
    .expect("Invalid base hour, minute, or second for NaiveDate");

/// Formats internal seconds representation (seconds since 1900-01-01) to a timestamp string.
pub fn hours_to_timestamp_string(hours: i64) -> String {
    (BASE_DATETIME_NAIVE + Duration::hours(hours))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

pub fn collect_input_files(
    input_path: &PathBuf,
) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    if !input_path.exists() {
        return Err(format!("Input path does not exist: {}", input_path.display()).into());
    }

    let mut input_files: Vec<PathBuf> = Vec::new();

    match (input_path.is_dir(), input_path.is_file()) {
        (true, false) => {
            for entry_result in walkdir::WalkDir::new(input_path) {
                let entry = entry_result?;
                if entry.file_type().is_file() {
                    if entry.path().extension().map_or(false, |ext| ext == "nc") {
                        input_files.push(entry.path().to_path_buf());
                    }
                }
            }
            if input_files.is_empty() {
                return Err(
                    format!("No .nc files found in directory: {}", input_path.display()).into(),
                );
            }
        }
        (false, true) => {
            if input_path.extension().map_or(false, |ext| ext == "nc") {
                input_files.push(input_path.to_path_buf());
            } else {
                return Err(
                    format!("Input file is not a .nc file: {}", input_path.display()).into(),
                );
            }
        }
        _ => {
            return Err(format!(
                "Input path is not a valid file or directory: {}",
                input_path.display()
            )
            .into());
        }
    }
    Ok(input_files)
}

pub fn extract_locations(dataset: &netcdf::File) -> Result<(Vec<f32>, Vec<f32>), Box<dyn Error>> {
    let lat_seq = dataset
        .variable("lat")
        .ok_or_else(|| "Missing 'lat' variable".to_string())?
        .get_values::<f32, _>(..)
        .map_err(|e| format!("Failed to read 'lat' variable: {}", e))?;

    let lon_seq = dataset
        .variable("lon")
        .ok_or_else(|| "Missing 'lon' variable".to_string())?
        .get_values::<f32, _>(..)
        .map_err(|e| format!("Failed to read 'lon' variable: {}", e))?;
    Ok((lat_seq, lon_seq))
}

pub fn write_data_to_csv(
    output_path: &PathBuf,
    variable_name: &str,
    data: &mut Vec<(i64, f64)>,
) -> Result<(), Box<dyn std::error::Error>> {
    if data.is_empty() {
        return Err(
            "No data extracted. All files/groups might have failed processing or contained no matching data."
                .into(),
        );
    }

    data.par_sort_unstable_by_key(|k| k.0);

    let file = File::create(output_path)?;
    let mut wtr = Writer::from_writer(file);

    wtr.write_record(&["timestamp", variable_name])?;

    data.iter().try_for_each(|(internal_ts, value)| {
        let timestamp_str = hours_to_timestamp_string(*internal_ts);
        wtr.write_record(&[timestamp_str, format!("{:.2}", value)])
    })?;

    wtr.flush()?;
    println!("Data successfully written to {}", output_path.display());
    Ok(())
}
