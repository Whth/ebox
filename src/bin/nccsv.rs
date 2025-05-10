use chrono::{Duration, NaiveDate};
use clap::{arg, Parser};
use csv::Writer;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs::File;
use std::path::{Path, PathBuf};

struct Point {
    lon: f32,
    lat: f32,
}

impl Point {
    fn new(lon: f32, lat: f32) -> Self {
        Point { lon, lat }
    }
    fn get_nearest_sample(&self, lat_seq: Vec<f32>, lon_seq: Vec<f32>) -> (usize, usize) {
        // Find the index of the latitude in lat_seq closest to self.lat
        let lat_idx = lat_seq
            .par_iter() // Use par_iter for parallel processing on Vec
            .enumerate()
            //.par_bridge() // Not needed if lat_seq is already a parallel iterator source
            .min_by_key(|&(_idx, &val_from_seq)| (val_from_seq - self.lat).abs().to_bits())
            .map(|(index, _)| index)
            .expect("lat_seq should not be empty and must contain valid float values.");

        // Find the index of the longitude in lon_seq closest to self.lon
        let lon_idx = lon_seq
            .par_iter() // Use par_iter for parallel processing on Vec
            .enumerate()
            //.par_bridge()
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
    /// This argument is required.
    #[arg(short, long, required = true)]
    lat: f32,

    /// Longitude for data extraction (in degrees_east).
    /// This argument is required.
    #[arg(short, long, required = true)]
    lon: f32,

    /// Name of the variable to extract from the NetCDF file.
    /// Defaults to "wind" if not specified.
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
    args: &Args,
    point: &Point,
) -> Result<Vec<(i64, f64)>, Box<dyn std::error::Error>> {
    let dataset = netcdf::open(file_path)?;

    let lat_var = dataset
        .variable("lat")
        .ok_or_else(|| format!("Missing 'lat' variable in {}", file_path.display()))?;
    let lon_var = dataset
        .variable("lon")
        .ok_or_else(|| format!("Missing 'lon' variable in {}", file_path.display()))?;
    let time_var = dataset
        .variable("time")
        .ok_or_else(|| format!("Missing 'time' variable in {}", file_path.display()))?;

    let lat_seq = lat_var
        .get_values::<f32, _>(..)
        .map_err(|e| format!("Failed to read 'lat' from {}: {}", file_path.display(), e))?;
    let lon_seq = lon_var
        .get_values::<f32, _>(..)
        .map_err(|e| format!("Failed to read 'lon' from {}: {}", file_path.display(), e))?;

    let (nearest_lat_idx, nearest_lon_idx) = point.get_nearest_sample(lat_seq, lon_seq);

    let data_var = dataset.variable(&args.variable).ok_or_else(|| {
        format!(
            "Missing '{}' variable in {}",
            args.variable,
            file_path.display()
        )
    })?;

    let dims = data_var.dimensions();
    if dims.len() < 3 {
        // Assuming dimensions are (time, lat, lon) or similar like (time, y, x)
        return Err(format!(
            "Variable '{}' in {} does not have enough dimensions (expected at least 3, got {})",
            args.variable,
            file_path.display(),
            dims.len()
        )
        .into());
    }

    // Validate indices against actual dimension lengths of the specific variable
    // Assuming standard (time, lat, lon) order for dimensions of the variable.
    // dim[0] is time, dim[1] is latitude-like, dim[2] is longitude-like.
    let lat_dim_len = dims.get(1).map_or(0, |d| d.len());
    let lon_dim_len = dims.get(2).map_or(0, |d| d.len());

    if nearest_lat_idx >= lat_dim_len {
        return Err(format!(
            "Latitude index {} out of bounds for variable '{}' in {} (lat_dim_len: {})",
            nearest_lat_idx,
            args.variable,
            file_path.display(),
            lat_dim_len
        )
        .into());
    }
    if nearest_lon_idx >= lon_dim_len {
        return Err(format!(
            "Longitude index {} out of bounds for variable '{}' in {} (lon_dim_len: {})",
            nearest_lon_idx,
            args.variable,
            file_path.display(),
            lon_dim_len
        )
        .into());
    }

    let values_array = data_var
        .get_values::<f64, _>((.., nearest_lat_idx, nearest_lon_idx))
        .map_err(|e| {
            format!(
                "Failed to read data for '{}' from {}: {}",
                args.variable,
                file_path.display(),
                e
            )
        })?;

    let time_values_array = time_var.get_values::<f64, _>(..).map_err(|e| {
        format!(
            "Failed to read 'time' variable from {}: {}",
            file_path.display(),
            e
        )
    })?;

    if values_array.len() != time_values_array.len() {
        return Err(format!(
            "Mismatch in number of data points ({}) and timestamps ({}) in {}",
            values_array.len(),
            time_values_array.len(),
            file_path.display()
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
            // Path is a directory
            for entry_result in walkdir::WalkDir::new(input_path) {
                let entry = entry_result?; // Propagate errors from WalkDir iterator
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
            // Path is a file
            if input_path.extension().map_or(false, |ext| ext == "nc") {
                input_files.push(input_path.to_path_buf());
            } else {
                return Err(format!("Input file is not a .nc file: {}", input_path_str).into());
            }
        }
        _ => {
            // Path exists but is neither a directory nor a regular file (e.g., symlink, or unexpected state)
            return Err(format!(
                "Input path is not a valid file or directory: {}",
                input_path_str
            )
            .into());
        }
    }

    // If we reach here, input_files must contain at least one valid path
    // because all error conditions (no .nc files in dir, not an .nc file, invalid path type)
    // would have caused an early return.
    // The original code had a final check: `if input_files.is_empty()`, which is now covered
    // by the specific error messages within the match arms.

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
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .unwrap()
            .progress_chars("#>-"),
    );

    input_files
        .par_iter()
        .progress_with(pb) // Integrate indicatif progress bar
        .filter_map(|file_path| {
            // println!("Processing file: {}", file_path.display()); // Replaced by progress bar
            match process_file(file_path, args, point) {
                Ok(data_from_file) => Some(data_from_file),
                Err(e) => {
                    // eprintln is still useful for errors, even with a progress bar
                    eprintln!("Skipping file {}: {}", file_path.display(), e);
                    None
                }
            }
        })
        .flatten() // Flattens Vec<Vec<(i64, f64)>> into an iterator of (i64, f64)
        .collect::<Vec<(i64, f64)>>()
}

fn write_data_to_csv(
    output_path_str: &str,
    variable_name: &str,
    data: &mut Vec<(i64, f64)>,
) -> Result<(), Box<dyn std::error::Error>> {
    if data.is_empty() {
        return Err(
            "No data extracted. All files might have failed processing or contained no matching data."
                .into(),
        );
    }

    data.par_sort_unstable_by_key(|k| k.0);

    let file = File::create(output_path_str)?;
    let mut wtr = Writer::from_writer(file);

    wtr.write_record(&["timestamp", variable_name])?;

    for (internal_ts, value) in data {
        let timestamp_str = internal_seconds_to_timestamp_string(*internal_ts);
        wtr.write_record(&[timestamp_str, value.to_string()])?;
    }

    wtr.flush()?;
    println!("Data successfully written to {}", output_path_str);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let input_files = collect_input_files(&args.input)?;

    let point = Point::new(args.lon, args.lat);

    let mut all_data = aggregate_data_from_files(&input_files, &args, &point);

    write_data_to_csv(&args.output, &args.variable, &mut all_data)?;

    Ok(())
}
