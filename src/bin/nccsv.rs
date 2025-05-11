use chrono::{Duration, NaiveDate};
use clap::{arg, Parser};
use csv::Writer;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
// No new imports seem strictly necessary, netcdf::File will be used qualified.

struct Point {
    lon: f32,
    lat: f32,
}

impl Point {
    fn new(lon: f32, lat: f32) -> Self {
        Point { lon, lat }
    }
    fn get_nearest_sample(&self, lat_seq: Vec<f32>, lon_seq: Vec<f32>) -> (usize, usize) {
        let lat_idx = lat_seq
            .par_iter()
            .enumerate()
            .min_by_key(|&(_idx, &val_from_seq)| (val_from_seq - self.lat).abs().to_bits())
            .map(|(index, _)| index)
            .expect("lat_seq should not be empty and must contain valid float values.");

        let lon_idx = lon_seq
            .par_iter()
            .enumerate()
            .min_by_key(|&(_idx, &val_from_seq)| (val_from_seq - self.lon).abs().to_bits())
            .map(|(index, _)| index)
            .expect("lon_seq should not be empty and must contain valid float values.");

        (lat_idx, lon_idx)
    }
}

/// Command Line Interface (CLI) arguments for the NetCDF to CSV conversion tool.
///
/// This program extracts data for a specific variable at a given latitude and longitude
/// from NetCDF file(s) and outputs it to a CSV file. If a directory is provided as input,
/// it processes all .nc files in that directory.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    input: String,

    #[arg(default_value = "output.csv")]
    output: String,

    /// Latitude for data extraction (in degrees_north).
    #[arg(short, long, required = true)]
    lat: f32,

    /// Longitude for data extraction (in degrees_east).
    #[arg(short, long, required = true)]
    lon: f32,

    /// Name of the variable to extract from the NetCDF file.
    #[arg(short, long, default_value = "wind")]
    variable: String,
}

/// Converts hours since 1900-01-01 to an internal representation (seconds since 1900-01-01).
fn hours_since_1900_to_internal_seconds(hours_since_1900: f64) -> i64 {
    (hours_since_1900 * 3600.0).round() as i64
}

/// Formats internal seconds representation (seconds since 1900-01-01) to a timestamp string.
fn internal_seconds_to_timestamp_string(seconds_since_1900: i64) -> String {
    let base_datetime_naive = NaiveDate::from_ymd_opt(1900, 1, 1)
        .expect("Invalid base year, month, or day for NaiveDate")
        .and_hms_opt(0, 0, 0)
        .expect("Invalid base hour, minute, or second for NaiveDate");

    let target_datetime_naive = base_datetime_naive + Duration::seconds(seconds_since_1900);

    target_datetime_naive
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn process_file(
    file_path: &Path,
    dataset_identifier: &str,
    args: &Args,
    point: &Point,
    cached_indices_arc: &Arc<Mutex<Option<(usize, usize)>>>,
) -> Result<Vec<(i64, f64)>, Box<dyn std::error::Error>> {
    let dataset = netcdf::open(file_path)
        .map_err(|e| format!("Failed to open NetCDF file '{}': {}", dataset_identifier, e))?;

    let (nearest_lat_idx, nearest_lon_idx);

    let mut indices_opt_guard = cached_indices_arc
        .lock()
        .map_err(|e| format!("Mutex for cached_indices poisoned: {}", e))?;

    if let Some(cached_idxs) = *indices_opt_guard {
        nearest_lat_idx = cached_idxs.0;
        nearest_lon_idx = cached_idxs.1;
        drop(indices_opt_guard);
    } else {
        let lat_seq = dataset
            .variable("lat")
            .ok_or_else(|| format!("Missing 'lat' variable in {}", dataset_identifier))?
            .get_values::<f32, _>(..)
            .map_err(|e| format!("Failed to read 'lat' from {}: {}", dataset_identifier, e))?;
        let lon_seq = dataset
            .variable("lon")
            .ok_or_else(|| format!("Missing 'lon' variable in {}", dataset_identifier))?
            .get_values::<f32, _>(..)
            .map_err(|e| format!("Failed to read 'lon' from {}: {}", dataset_identifier, e))?;

        let (idx_lat, idx_lon) = point.get_nearest_sample(lat_seq, lon_seq);

        *indices_opt_guard = Some((idx_lat, idx_lon));
        nearest_lat_idx = idx_lat;
        nearest_lon_idx = idx_lon;
    }

    let time_var = dataset
        .variable("time")
        .ok_or_else(|| format!("Missing 'time' variable in {}", dataset_identifier))?;
    let data_var = dataset.variable(&args.variable).ok_or_else(|| {
        format!(
            "Missing '{}' variable in {}",
            args.variable, dataset_identifier
        )
    })?;

    let dims = data_var.dimensions();
    if dims.len() < 3 {
        return Err(format!(
            "Variable '{}' in {} has insufficient dimensions (expected >=3, got {})",
            args.variable,
            dataset_identifier,
            dims.len()
        )
        .into());
    }

    let lat_dim_len = dims.get(1).map_or(0, |d| d.len());
    let lon_dim_len = dims.get(2).map_or(0, |d| d.len());

    if nearest_lat_idx >= lat_dim_len {
        return Err(format!(
            "Latitude index {} out of bounds for {} (lat_dim_len: {})",
            nearest_lat_idx, dataset_identifier, lat_dim_len
        )
        .into());
    }
    if nearest_lon_idx >= lon_dim_len {
        return Err(format!(
            "Longitude index {} out of bounds for {} (lon_dim_len: {})",
            nearest_lon_idx, dataset_identifier, lon_dim_len
        )
        .into());
    }

    let values_array = data_var
        .get_values::<f64, _>((.., nearest_lat_idx, nearest_lon_idx))
        .map_err(|e| format!("Failed to read data from {}: {}", dataset_identifier, e))?;

    let time_values_array = time_var
        .get_values::<f64, _>(..)
        .map_err(|e| format!("Failed to read 'time' from {}: {}", dataset_identifier, e))?;

    if values_array.len() != time_values_array.len() {
        return Err(format!(
            "Mismatch in data points/timestamps in {} ({} vs {})",
            dataset_identifier,
            values_array.len(),
            time_values_array.len()
        )
        .into());
    }

    let mut file_data = Vec::with_capacity(values_array.len());
    for (idx, &value) in values_array.iter().enumerate() {
        let raw_time = time_values_array[idx];
        let internal_ts = hours_since_1900_to_internal_seconds(raw_time);
        file_data.push((internal_ts, value));
    }

    Ok(file_data)
}

fn collect_input_files(input_path_str: &str) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    let input_path = Path::new(input_path_str);

    if !input_path.exists() {
        return Err(format!("Input path does not exist: {}", input_path_str).into());
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
                return Err(format!("No .nc files found in directory: {}", input_path_str).into());
            }
        }
        (false, true) => {
            if input_path.extension().map_or(false, |ext| ext == "nc") {
                input_files.push(input_path.to_path_buf());
            } else {
                return Err(format!("Input file is not a .nc file: {}", input_path_str).into());
            }
        }
        _ => {
            return Err(format!(
                "Input path is not a valid file or directory: {}",
                input_path_str
            )
            .into());
        }
    }
    Ok(input_files)
}

fn aggregate_data_from_files(
    input_files: &[PathBuf],
    args: &Args,
    point: &Point,
) -> Vec<(i64, f64)> {
    let pb = ProgressBar::new(input_files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] Processing file {pos}/{len} ({eta})",
            )
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("#>-")
    );

    let cached_indices = Arc::new(Mutex::new(None::<(usize, usize)>));

    input_files
        .par_iter()
        .progress_with(pb)
        .filter_map(|file_path| {
            let dataset_identifier = file_path.display().to_string();

            match process_file(file_path, &dataset_identifier, args, point, &cached_indices) {
                Ok(data_from_file) => Some(data_from_file),
                Err(e) => {
                    eprintln!("Error processing {}: {}", dataset_identifier, e);
                    None
                }
            }
        })
        .flatten()
        .collect::<Vec<(i64, f64)>>()
}

fn write_data_to_csv(
    output_path_str: &str,
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

    let file = File::create(output_path_str)?;
    let mut wtr = Writer::from_writer(file);

    wtr.write_record(&["timestamp", variable_name])?;

    for (internal_ts, value) in data {
        let timestamp_str = internal_seconds_to_timestamp_string(*internal_ts);
        wtr.write_record(&[timestamp_str, format!("{:.2}", value)])?;
    }

    wtr.flush()?;
    println!("Data successfully written to {}", output_path_str);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let input_files = collect_input_files(&args.input)?;
    if input_files.is_empty() {
        // collect_input_files already returns an error if no files are found or input is invalid.
        // So, this state should ideally not be reached if collect_input_files is robust.
        // If it could return Ok(empty_vec), then:
        eprintln!("No input .nc files to process.");
        return Ok(()); // Or an error
    }

    let point = Point::new(args.lon, args.lat);

    let mut all_data = aggregate_data_from_files(&input_files, &args, &point);

    write_data_to_csv(&args.output, &args.variable, &mut all_data)?;

    Ok(())
}
