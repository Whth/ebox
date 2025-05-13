use clap::{Args, Parser, Subcommand};
use indicatif::{ParallelProgressIterator, ProgressBar};
use kmeans::*;
use ndarray::{Array1, Array2};
use polars::prelude::*;
use rand::prelude::*;
use rayon::prelude::*;
use scirs2_metrics::clustering;
use std::path::PathBuf;

#[derive(Parser)]
#[command(author, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Nop(Nop),
}

#[derive(Debug, Args)]
struct Nop {
    input: PathBuf,

    #[arg(short, long, default_value = "wind")]
    wind_field: String,

    #[arg(short, long, default_value_t = 4000)]
    sample_count: u32,

    #[arg(short, long, default_value_t = 2)]
    start: u32,
    #[arg(short, long, default_value_t = 7)]
    end: u32,

    #[arg(short = 'S', long, default_value_t = 0.34)]
    weight_s: f32,
    #[arg(short = 'C', long, default_value_t = 0.33)]
    weight_ch: f32,
    #[arg(short = 'D', long, default_value_t = 0.33)]
    weight_db: f32,

    #[arg(short, long, default_value = "output_scores.csv")]
    output: PathBuf,
}

fn probability_norm(seq: Vec<f64>) -> Vec<f64> {
    let sum: f64 = seq.iter().sum();
    seq.iter().map(|x| x / sum).collect()
}

fn opt_best_n_state(arg: Nop) -> Result<(), Box<dyn std::error::Error>> {
    let data: Vec<f64> = LazyCsvReader::new(arg.input)
        .with_has_header(true)
        .finish()?
        .select([col(&arg.wind_field)])
        .collect()?
        .column(&arg.wind_field)?
        .f64()?
        .to_vec()
        .iter()
        .map(|x| x.expect("Bad value!"))
        .collect();
    println!("Read {} data points.", data.len());

    let kmean: KMeans<f64, 16, _> = KMeans::new(&data, data.len(), 1, EuclideanDistance);

    let task: Vec<u32> = (arg.start..arg.end).collect();

    let scores = task
        .par_iter()
        .progress_with(ProgressBar::new(task.len() as u64))
        .map(|&n| {
            let res = kmean.kmeans_lloyd(
                n as usize,
                u64::MAX as usize,
                KMeans::init_kmeanplusplus,
                &KMeansConfig::default(),
            );

            let sample_limit = arg.sample_count as usize;
            let (data_for_silhouette, assignments_for_silhouette, n_samples_silhouette): (
                Vec<f64>,
                Vec<usize>,
                usize,
            );

            if data.len() > sample_limit {
                // Data size exceeds limit, perform random sampling
                // Assumes `use rand::thread_rng;` and `use rand::seq::SliceRandom;` are present or IDE will add them.
                let mut rng = thread_rng();
                let all_indices: Vec<usize> = (0..data.len()).collect();

                let chosen_indices: Vec<usize> = all_indices
                    .choose_multiple(&mut rng, sample_limit)
                    .cloned()
                    .collect();

                data_for_silhouette = chosen_indices.iter().map(|&i| data[i]).collect();
                assignments_for_silhouette =
                    chosen_indices.iter().map(|&i| res.assignments[i]).collect();
                n_samples_silhouette = sample_limit;
                // println!("Data length ({}) > sample_count ({}). Randomly sampled {} points for Silhouette score.", data.len(), sample_limit, n_samples_silhouette);
            } else {
                // Data size is within limit (or equal), use all data
                data_for_silhouette = data.clone();
                assignments_for_silhouette = res.assignments.clone();
                n_samples_silhouette = data.len();
                // println!("Data length ({}) <= sample_count ({}). Using all {} points for Silhouette score.", data.len(), sample_limit, n_samples_silhouette);
            }

            let silhouette = clustering::silhouette_score(
                &Array2::from_shape_vec(
                    (n_samples_silhouette, 2), // Use the actual number of samples for silhouette
                    data_for_silhouette
                        .iter()
                        .flat_map(|&x| vec![x, 0.0])
                        .collect(),
                )
                .expect("Bad shape"),
                &Array1::from_vec(assignments_for_silhouette), // Use the (potentially sampled) assignments
                "euclidean",
            )
            .expect("Bad shape");

            let calinski_harabasz = clustering::calinski_harabasz_score(
                &Array2::from_shape_vec(
                    (data.len(), 2),
                    data.iter().flat_map(|&x| vec![x, 0.0]).collect(),
                )
                .expect("Bad shape"),
                &Array1::from_vec(res.assignments.clone()),
            )
            .expect("Bad shape");
            let davies_bouldin = clustering::davies_bouldin_score(
                &Array2::from_shape_vec(
                    (data.len(), 2),
                    data.iter().flat_map(|&x| vec![x, 0.0]).collect(),
                )
                .expect("Bad shape"),
                &Array1::from_vec(res.assignments),
            )
            .expect("Bad shape");
            (n, silhouette, calinski_harabasz, davies_bouldin)
        })
        .collect::<Vec<(u32, f64, f64, f64)>>();

    let silhouette_scores: Vec<f64> = scores.iter().map(|(_, s, _, _)| *s).collect();
    let calinski_harabasz_scores: Vec<f64> = scores.iter().map(|(_, _, ch, _)| *ch).collect();
    let davies_bouldin_scores: Vec<f64> = scores.iter().map(|(_, _, _, db)| *db).collect();

    let silhouette_weights =
        probability_norm(silhouette_scores.iter().map(|&x| x as f64).collect());
    let calinski_harabasz_weights =
        probability_norm(calinski_harabasz_scores.iter().map(|&x| x as f64).collect());
    let davies_bouldin_weights =
        probability_norm(davies_bouldin_scores.iter().map(|&x| x as f64).collect());

    let n_values: Vec<u32> = scores.iter().map(|(n, _, _, _)| *n).collect();

    let total_scores_calculated: Vec<f64> = scores
        .iter()
        .enumerate()
        .map(|(i, (_, s, ch, db))| {
            s * arg.weight_s as f64 * silhouette_weights[i]
                + ch * arg.weight_ch as f64 * calinski_harabasz_weights[i]
                + (1.0 - db) * arg.weight_db as f64 * davies_bouldin_weights[i]
        })
        .collect();

    let mut df = DataFrame::new(vec![
        Column::new("n".into(), n_values),
        Column::new("silhouette_score".into(), silhouette_weights),
        Column::new("calinski_harabasz_score".into(), calinski_harabasz_weights),
        Column::new("davies_bouldin_score".into(), davies_bouldin_weights),
        Column::new("total_score".into(), total_scores_calculated),
    ])?;

    let file = std::fs::File::create(arg.output.clone())?;
    CsvWriter::new(file).include_header(true).finish(&mut df)?;

    println!("Scores dumped to {}", arg.output.display());
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Nop(arg) => opt_best_n_state(arg)?,
    }

    Ok(())
}
