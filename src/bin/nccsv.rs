use chrono::{Duration, NaiveDate};
use clap::{arg, Parser};
use csv::Writer;
use rayon::prelude::*;
use std::fs::File;
use std::path::Path;

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
            .iter()
            .enumerate()
            .par_bridge()
            .min_by_key(|&(_idx, &val_from_seq)| (val_from_seq - self.lat).abs().to_bits())
            .map(|(index, _)| index)
            .expect("lat_seq should not be empty and must contain valid float values.");

        // Find the index of the longitude in lon_seq closest to self.lon
        let lon_idx = lon_seq
            .iter()
            .enumerate()
            .par_bridge()
            .min_by_key(|&(_idx, &val_from_seq)| (val_from_seq - self.lon).abs().to_bits())
            .map(|(index, _)| index)
            .expect("lon_seq should not be empty and must contain valid float values.");

        (lat_idx, lon_idx)
    }
}

/// Command Line Interface (CLI) arguments for the NetCDF to CSV conversion tool.
///
/// This program extracts data for a specific variable at a given latitude and longitude
/// from a NetCDF file and outputs it to a CSV file.
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

fn to_timestamp(hours_since_1900: f64) -> String {
    // Base datetime: 1900-01-01 00:00:0.0.
    // NaiveDate represents a date without a timezone.
    let base_datetime_naive = NaiveDate::from_ymd_opt(1900, 1, 1)
        .expect("Invalid base year, month, or day for NaiveDate")
        .and_hms_opt(0, 0, 0)
        .expect("Invalid base hour, minute, or second for NaiveDate");

    // Convert the input hours (since 1900-01-01) to seconds.
    // 1 hour = 3600 seconds.
    // Rounding handles potential floating-point inaccuracies before casting to i64.
    let offset_seconds = (hours_since_1900 * 3600.0).round() as i64;

    // Calculate the target datetime by adding the offset (as a Duration) to the base NaiveDateTime.
    // Duration::seconds assumes `use chrono::Duration;` is present or will be added.
    let target_datetime_naive = base_datetime_naive + Duration::seconds(offset_seconds);

    // Format the NaiveDateTime into a long string pattern, e.g., "YYYY-MM-DD HH:MM:SS".
    target_datetime_naive
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let dataset = netcdf::open(Path::new(&args.input))?;

    let lat_var = dataset.variable("lat").expect("Missing 'lat' variable");
    let lon_var = dataset.variable("lon").expect("Missing 'lon' variable");
    let time_var = dataset.variable("time").expect("Missing 'time' variable");
    let point = Point::new(args.lon, args.lat);
    let (nearest_lat_idx, nearest_lon_idx) = point.get_nearest_sample(
        lat_var
            .get_values::<f32, _>(..)
            .expect("Failed to read 'lat' variable"),
        // Corrected order based on Point impl
        lon_var
            .get_values::<f32, _>(..)
            .expect("Failed to read 'lon' variable"),
    );

    let data_var = dataset
        .variable(args.variable.as_str())
        .ok_or_else(|| format!("Missing '{}' variable", args.variable))?;

    // Ensure the indices are within bounds for the data variable dimensions
    let dims = data_var.dimensions();
    if dims.len() < 3 {
        // Assuming time, lat, lon or time, y, x
        return Err(format!(
            "Variable '{}' does not have enough dimensions (expected at least 3)",
            args.variable
        )
        .into());
    }
    // Typically, NetCDF order is (time, lat, lon) or (time, y, x)
    // The order from get_nearest_sample is (lat_idx, lon_idx)
    // So we access data as (.., nearest_lat_idx, nearest_lon_idx)
    // If the variable has other dimensions or a different order, this needs adjustment.
    // For example, if it's (time, lon, lat), then it should be (.., nearest_lon_idx, nearest_lat_idx)
    // The current code assumes (time, lat, lon) based on the original variable names 'lat' and 'lon' for indexing.
    // The original code had `(.., nearest_lon_idx, nearest_lat_idx)`, which implies (time, some_dim_for_lon, some_dim_for_lat)
    // Let's stick to what seems implied by the original indexing logic.
    // `get_nearest_sample` returns (lat_idx, lon_idx).
    // If the NetCDF variable is (time, lat_dim, lon_dim), then access should be (.., lat_idx, lon_idx).
    // If the NetCDF variable is (time, lon_dim, lat_dim), then access should be (.., lon_idx, lat_idx).
    // The original code had `wind_var.get_values::<f64, _>((.., nearest_lon_idx, nearest_lat_idx))`
    // and `get_nearest_sample` returns `(lat_idx, lon_idx)`. This means `nearest_lon_idx` from the tuple
    // corresponds to the *second* spatial dimension in the `get_values` call, and `nearest_lat_idx` to the *third*.
    // This is a bit confusing. Let's assume the variable is (time, lat_dim_index, lon_dim_index)
    // And `get_nearest_sample` returns `(lat_index_for_lat_dim, lon_index_for_lon_dim)`
    // So access should be `(.., lat_index_for_lat_dim, lon_index_for_lon_dim)`.

    let values = data_var
        .get_values::<f64, _>((.., nearest_lat_idx, nearest_lon_idx)) // Adjusted based on tuple destructuring
        .map_err(|e| format!("Failed to read '{}' variable: {}", args.variable, e))?;

    let time_stamps: Vec<String> = time_var
        .get_values::<f64, _>(..)
        .expect("Failed to read 'time' variable")
        .iter()
        .par_bridge()
        .map(|&x| to_timestamp(x))
        .collect();

    let file = File::create(&args.output)?;
    let mut wtr = Writer::from_writer(file);

    // Write header
    wtr.write_record(&["Timestamp", args.variable.as_str()])?;

    // Write data
    for (value, timestamp) in values.iter().zip(time_stamps) {
        wtr.write_record(&[timestamp.to_string(), value.to_string()])?;
    }

    wtr.flush()?;
    println!("Data successfully written to {}", args.output);

    Ok(())
}
