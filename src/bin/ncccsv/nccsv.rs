mod utils;

use crate::utils::extract_locations;
use clap::{arg, Args as ClapArgs, Parser, Subcommand};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use ndarray::prelude::*;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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

/// Command Line Interface (CLI) for NetCDF data processing.
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "NetCDF data processing utility.",
    long_about = "A command-line tool to perform various operations on NetCDF files, such as extracting time series data to CSV."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Extracts time series data for a specific variable and geographic point from NetCDF files.
    ///
    /// This command processes one or more NetCDF files (either a single file path
    /// or all .nc files in a directory). It extracts time series data for a
    /// specified variable at the nearest grid point to the given latitude and
    /// longitude. The aggregated data is then written to a CSV file.
    Extract(ExtractArgs),

    /// Calculates and displays the average value of a specified variable from NetCDF file(s).
    Probe(ProbeArgs),
}

#[derive(ClapArgs, Debug)]
struct ProbeArgs {
    /// Path to the input NetCDF file.
    input: PathBuf,

    /// Path to the output CSV file.
    #[arg(default_value = "output.csv")]
    output: PathBuf,

    /// Latitude for data extraction (in degrees_north).
    #[arg(short = 'a', long, required = true)]
    lat: f32,

    /// Longitude for data extraction (in degrees_east).
    #[arg(short, long, required = true)]
    lon: f32,

    /// Name of the variable to probe from the NetCDF file.
    #[arg(short, long, default_value = "wind")]
    variable: String,
}

/// Arguments for the 'extract' subcommand.
#[derive(ClapArgs, Debug)]
struct ExtractArgs {
    /// Path to the input NetCDF file or directory containing .nc files.
    input: PathBuf,

    /// Path to the output CSV file.
    #[arg(default_value = "output.csv")]
    output: PathBuf,

    /// Latitude for data extraction (in degrees_north).
    #[arg(short = 'a', long, required = true)]
    lat: f32,

    /// Longitude for data extraction (in degrees_east).
    #[arg(short, long, required = true)]
    lon: f32,

    /// Name of the variable to extract from the NetCDF file.
    #[arg(short, long, default_value = "wind")]
    variable: String,
}

fn process_file(
    file_path: &Path,
    args: &ExtractArgs,
    point: &Point,
    cached_indices_arc: &Arc<Mutex<Option<(usize, usize)>>>,
) -> Result<Vec<(i64, f64)>, Box<dyn std::error::Error>> {
    let dataset = netcdf::open(file_path).map_err(|e| {
        format!(
            "Failed to open NetCDF file '{}': {}",
            file_path.display(),
            e
        )
    })?;

    let (nearest_lat_idx, nearest_lon_idx);

    let mut indices_opt_guard = cached_indices_arc
        .lock()
        .map_err(|e| format!("Mutex for cached_indices poisoned: {}", e))?;

    if let Some(cached_idxs) = *indices_opt_guard {
        nearest_lat_idx = cached_idxs.0;
        nearest_lon_idx = cached_idxs.1;
        drop(indices_opt_guard); // Release lock early
    } else {
        let (lat_seq, lon_seq) = utils::extract_locations(&dataset)?;

        let (idx_lat, idx_lon) = point.get_nearest_sample(lat_seq, lon_seq);

        *indices_opt_guard = Some((idx_lat, idx_lon));
        nearest_lat_idx = idx_lat;
        nearest_lon_idx = idx_lon;
        // MutexGuard is dropped automatically here when it goes out of scope
    }

    let time_var = dataset
        .variable("time")
        .ok_or_else(|| format!("Missing 'time' variable in {}", file_path.display()))?;
    let data_var = dataset.variable(&args.variable).ok_or_else(|| {
        format!(
            "Missing '{}' variable in {}",
            args.variable,
            file_path.display()
        )
    })?;

    let dims = data_var.dimensions();
    if dims.len() < 3 {
        return Err(format!(
            "Variable '{}' in {} has insufficient dimensions (expected >=3, got {})",
            args.variable,
            file_path.display(),
            dims.len()
        )
        .into());
    }

    // Assuming dimensions are (time, lat, lon) or similar
    let lat_dim_idx = dims
        .iter()
        .position(|d| d.name().to_lowercase() == "lat" || d.name().to_lowercase() == "latitude")
        .unwrap_or(1); // Fallback to index 1 if not named 'lat'/'latitude'
    let lon_dim_idx = dims
        .iter()
        .position(|d| d.name().to_lowercase() == "lon" || d.name().to_lowercase() == "longitude")
        .unwrap_or(2); // Fallback to index 2

    let lat_dim_len = dims.get(lat_dim_idx).map_or(0, |d| d.len());
    let lon_dim_len = dims.get(lon_dim_idx).map_or(0, |d| d.len());

    if nearest_lat_idx >= lat_dim_len {
        return Err(format!(
            "Latitude index {} out of bounds for {} (lat_dim_len: {})",
            nearest_lat_idx,
            file_path.display(),
            lat_dim_len
        )
        .into());
    }
    if nearest_lon_idx >= lon_dim_len {
        return Err(format!(
            "Longitude index {} out of bounds for {} (lon_dim_len: {})",
            nearest_lon_idx,
            file_path.display(),
            lon_dim_len
        )
        .into());
    }

    let values_array = data_var
        .get_values::<f64, _>((.., nearest_lat_idx, nearest_lon_idx)) // This slicing assumes (time, lat, lon) or (..., lat, lon)
        .map_err(|e| format!("Failed to read data from {}: {}", file_path.display(), e))?;

    let time_values_array = time_var
        .get_values::<f64, _>(..)
        .map_err(|e| format!("Failed to read 'time' from {}: {}", file_path.display(), e))?;

    if values_array.len() != time_values_array.len() {
        return Err(format!(
            "Mismatch in data points/timestamps in {} ({} vs {})",
            file_path.display(),
            values_array.len(),
            time_values_array.len()
        )
        .into());
    }

    let mut file_data = Vec::with_capacity(values_array.len());
    for (idx, &value) in values_array.iter().enumerate() {
        let raw_time = time_values_array[idx];

        file_data.push((raw_time as i64, value));
    }

    Ok(file_data)
}

fn aggregate_data_from_files(
    input_files: &[PathBuf],
    args: &ExtractArgs, // Changed from Args to ExtractArgs
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
        .filter_map(
            |file_path| match process_file(file_path, args, point, &cached_indices) {
                Ok(data_from_file) => Some(data_from_file),
                Err(e) => {
                    eprintln!("Error processing {}: {}", file_path.display(), e);
                    None
                }
            },
        )
        .flatten()
        .collect::<Vec<(i64, f64)>>()
}

fn handle_extract_command(args: &ExtractArgs) -> Result<(), Box<dyn std::error::Error>> {
    let input_files = utils::collect_input_files(&args.input)?;
    // collect_input_files is expected to return an Err if no suitable files are found,
    // so an explicit check for input_files.is_empty() is not performed here.

    let point = Point::new(args.lon, args.lat);

    let mut all_data = aggregate_data_from_files(&input_files, args, &point);

    utils::write_data_to_csv(&args.output, &args.variable, &mut all_data)?;

    Ok(())
}

fn handle_probe_command(args: &ProbeArgs) -> Result<(), Box<dyn std::error::Error>> {
    if args.input.is_dir() {
        return Err(format!(
            "Input path is not a valid file or directory: {}",
            args.input.display()
        )
        .into());
    }

    let point = Point::new(args.lon, args.lat);

    let dataset = netcdf::open(&args.input)?;

    let (lat_seq, lon_seq) = extract_locations(&dataset)?;

    let (nearest_lat_idx, nearest_lon_idx) = point.get_nearest_sample(lat_seq, lon_seq);

    let arr = Array1::from(
        dataset
            .variable(args.variable.as_str())
            .ok_or("Variable not found")?
            .get_values::<f64, _>((.., nearest_lat_idx, nearest_lon_idx))?,
    );

    println!(
        "Statistics for variable '{}' at point (Lat: {:.2}, Lon: {:.2}):",
        args.variable, args.lat, args.lon
    );
    println!("  Total data points retrieved: {}", arr.len());

    // Filter out NaN or infinite values for meaningful statistics
    let finite_values: Vec<f64> = arr
        .par_iter()
        .filter(|&&x| x.is_finite())
        .cloned()
        .collect();

    println!("  Finite data points: {}", finite_values.len());

    if finite_values.is_empty() {
        println!("  No finite data points available to calculate statistics.");
    } else {
        // Create a new Array1 from finite values to use ndarray's statistical methods
        let finite_arr = Array1::from_vec(finite_values);

        // Calculate mean
        // For a non-empty array of finite numbers, mean() will return Some(value).
        let mean_val = finite_arr
            .mean()
            .expect("Mean calculation failed for non-empty finite array");

        // Calculate standard deviation (population standard deviation, ddof=0)
        // std(0.0) is NaN if the array is empty, but we've guarded against that.
        // If the array has one element, std(0.0) is 0.0.
        let std_dev_val = finite_arr.std(0.0);

        // Calculate min and max
        // These are safe because finite_arr is guaranteed not to be empty and contains only finite numbers.
        let min_val = finite_arr.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max_val = finite_arr.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));

        println!("  ------------------------------------");
        println!("  Mean:           {:.4}", mean_val);
        println!("  Std Deviation:  {:.4}", std_dev_val);
        println!("  Minimum:        {:.4}", min_val);
        println!("  Maximum:        {:.4}", max_val);
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Extract(args) => {
            handle_extract_command(args)?;
        }
        Commands::Probe(args) => {
            handle_probe_command(args)?;
        }
    }

    Ok(())
}
