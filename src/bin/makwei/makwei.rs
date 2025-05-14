mod utils;

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

    #[arg(short = 'c', long, default_value_t = 4000)]
    sample_count: u32,

    #[arg(short, long, default_value_t = 2)]
    start: u32,
    #[arg(short, long, default_value_t = 7)]
    end: u32,

    #[arg(short = 'S', long, default_value_t = 0.34)]
    weight_s: f64,
    #[arg(short = 'C', long, default_value_t = 0.33)]
    weight_ch: f64,
    #[arg(short = 'D', long, default_value_t = 0.33)]
    weight_db: f64,

    #[arg(short = 'N', long, default_value = "probability")]
    norm_method: String,

    #[arg(short, long, default_value = "output_scores.csv")]
    output: PathBuf,
}

#[derive(Clone, Copy)]
enum IndicatorType {
    Positive,
    Negative,
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

    // Apply normalization based on user choice
    let (silhouette_weights, calinski_harabasz_weights, davies_bouldin_weights) =
        match arg.norm_method.as_str() {
            "probability" => (
                utils::probability_norm(silhouette_scores.clone()),
                utils::probability_norm(calinski_harabasz_scores.clone()),
                utils::probability_norm(davies_bouldin_scores.clone()),
            ),
            "minmax" => (
                utils::min_max_norm(silhouette_scores.clone()),
                utils::min_max_norm(calinski_harabasz_scores.clone()),
                utils::min_max_norm_rev(davies_bouldin_scores.clone()),
            ),
            "scale" => (
                utils::scale_norm(silhouette_scores.clone()),
                utils::scale_norm(calinski_harabasz_scores.clone()),
                utils::scale_norm(davies_bouldin_scores.clone()),
            ),
            "zscore" => (
                utils::z_score_norm(silhouette_scores.clone()),
                utils::z_score_norm(calinski_harabasz_scores.clone()),
                utils::z_score_norm(davies_bouldin_scores.clone()),
            ),
            _ => panic!("Unsupported normalization method: {}", arg.norm_method),
        };

    let n_values: Vec<u32> = scores.iter().map(|(n, _, _, _)| *n).collect();

    let s_devi = utils::calculate_variance(silhouette_weights.clone());
    let c_devi = utils::calculate_variance(calinski_harabasz_weights.clone());
    let d_devi = utils::calculate_variance(davies_bouldin_weights.clone());
    let ve = utils::probability_norm(vec![s_devi, c_devi, d_devi]);
    println!("The weights : {:?}", ve);

    let weights = utils::entropy_weight_method(
        &[
            silhouette_scores,
            calinski_harabasz_scores,
            davies_bouldin_scores,
        ],
        &[
            IndicatorType::Positive,
            IndicatorType::Positive,
            IndicatorType::Negative,
        ],
    );
    println!("The entropy weights : {:?}", weights);

    let total_scores_calculated: Vec<f64> = silhouette_weights
        .iter()
        .map(|&w| w * weights[0])
        .zip(calinski_harabasz_weights.iter())
        .map(|(w, w2)| w + w2 * weights[1])
        .zip(davies_bouldin_weights.iter())
        .map(|(w, w2)| w + w2 * weights[2])
        .collect();

    let mut df = DataFrame::new(vec![
        Column::new("n".into(), n_values),
        Column::new("silhouette_score".into(), silhouette_weights),
        Column::new("calinski_harabasz_score".into(), calinski_harabasz_weights),
        Column::new("davies_bouldin_score".into(), davies_bouldin_weights),
        Column::new("total_score".into(), total_scores_calculated),
    ])?;

    let file = std::fs::File::create(arg.output.clone())?;
    CsvWriter::new(file)
        .include_header(true)
        .with_float_precision(Some(3))
        .finish(&mut df)?;

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
