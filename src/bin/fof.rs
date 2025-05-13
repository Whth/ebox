use anyhow::Result;
use clap::Parser;
use polars::prelude::*;

/// CLI to filter wing data based on customizable performance criteria
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to input CSV file
    input: String,

    /// Path to output CSV file
    #[arg(short, long, default_value = "filtered.csv")]
    output: String,

    /// Minimum lift-to-drag ratio (C_L / C_D)
    #[arg(long, default_value_t = 5.0)]
    min_lift_drag_ratio: f64,

    /// Minimum lift coefficient (C_L)
    #[arg(long, default_value_t = 0.2)]
    min_lift: f64,

    /// Maximum drag coefficient (C_D)
    #[arg(long, default_value_t = 0.15)]
    max_drag: f64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Read CSV into DataFrame
    let df = LazyCsvReader::new(&args.input)
        .with_has_header(true)
        .with_dtype_overwrite(Some(SchemaRef::new(Schema::from_iter(vec![(
            PlSmallStr::from_str("naca_code"),
            DataType::String,
        )]))))
        .finish()?;

    // Apply filters using Polars expressions
    let mut filtered_df = df
        .lazy()
        .filter(
            (col("cl_at_best_aoa") / col("cd_at_best_aoa"))
                .gt_eq(lit(args.min_lift_drag_ratio))
                .and(col("cl_at_best_aoa").gt_eq(lit(args.min_lift)))
                .and(col("cd_at_best_aoa").lt_eq(lit(args.max_drag))),
        )
        .collect()?;

    // Write result to output CSV
    CsvWriter::new(std::fs::File::create(args.output)?)
        .include_header(true)
        .finish(&mut filtered_df)?;

    println!("Filtering completed successfully!");

    Ok(())
}
