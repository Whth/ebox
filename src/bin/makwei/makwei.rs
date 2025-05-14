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

/// 计算输入数值向量的方差。
///
/// # 参数
/// - `data`: 一个包含f64类型元素的向量，代表需要计算方差的数据集。
///
/// # 返回值
/// 返回一个f64类型的值，表示输入数据的方差。
fn calculate_variance(data: Vec<f64>) -> f64 {
    let mean = data.iter().sum::<f64>() / data.len() as f64;
    (data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / data.len() as f64).sqrt()
}
/// 对输入的数值序列进行概率归一化处理，将序列转换为总和为1的概率分布。
///
/// # 参数
/// - `seq`: 一个包含f64类型元素的向量，代表需要归一化的数值序列。
///
/// # 返回值
/// 一个新的向量，其中每个元素是原序列中对应元素与序列总和的比值，使得新序列的总和为1。
fn probability_norm(seq: Vec<f64>) -> Vec<f64> {
    let sum: f64 = seq.iter().sum();
    seq.iter().map(|x| x / sum).collect()
}

/// 对输入的数值序列进行最小-最大归一化处理，将序列映射到[0, 1]区间。
///
/// # 参数
/// - `seq`: 一个包含f64类型元素的向量，代表需要归一化的数值序列。
///
/// # 返回值
/// 一个新的向量，其中每个元素被线性变换后落在[0, 1]区间内。
fn min_max_norm(seq: Vec<f64>) -> Vec<f64> {
    let max = seq.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min = seq.iter().cloned().fold(f64::INFINITY, f64::min);
    seq.iter().map(|x| (x - min) / (max - min)).collect()
}
fn min_max_norm_rev(seq: Vec<f64>) -> Vec<f64> {
    let max = seq.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min = seq.iter().cloned().fold(f64::INFINITY, f64::min);
    seq.iter().map(|x| (max - x) / (max - min)).collect()
}

/// 对输入的数值序列进行最大值归一化处理，将序列映射到[0, 1]区间。
///
/// # 参数
/// - `seq`: 一个包含f64类型元素的向量，代表需要归一化的数值序列。
///
/// # 返回值
/// 一个新的向量，其中每个元素是原序列中对应元素与序列最大值的比值，使得新序列的最大值为1。
fn scale_norm(seq: Vec<f64>) -> Vec<f64> {
    let max_val = seq
        .iter()
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .expect("No elements in sequence.");
    seq.iter().map(|x| x / max_val).collect()
}

/// 对输入的数值序列进行Z-score标准化处理，将序列转换为均值为0、标准差为1的标准正态分布形式。
///
/// # 参数
/// - `seq`: 一个包含f64类型元素的向量，代表需要标准化的数值序列。
///
/// # 返回值
/// 一个新的向量，其中每个元素是原序列中对应元素减去均值后再除以标准差的结果。
fn z_score_norm(seq: Vec<f64>) -> Vec<f64> {
    let mean = seq.iter().sum::<f64>() / seq.len() as f64;
    let std_dev = (seq.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / seq.len() as f64).sqrt();
    seq.iter().map(|x| (x - mean) / std_dev).collect()
}

#[derive(Clone, Copy)]
enum IndicatorType {
    Positive,
    Negative,
}

fn entropy_weight_method(data: &[Vec<f64>], types: &[IndicatorType]) -> Vec<f64> {
    let num_indicators = data.len();
    if num_indicators == 0 {
        return Vec::new();
    }

    // Assuming all inner Vecs (indicators) have the same number of samples.
    // This should be guaranteed by the caller or checked beforehand.
    let num_samples = data[0].len();

    if num_samples == 0 {
        // No samples, cannot compute weights. Return a vector of zeros or handle as error.
        return vec![0.0; num_indicators];
    }

    if num_samples == 1 {
        // With only one sample, all indicators are non-differentiating.
        // Standard practice: entropy is 0, divergence is 1. All indicators get equal weight.
        return vec![1.0 / num_indicators as f64; num_indicators];
    }

    let epsilon = 1e-12; // Small constant for floating point comparisons and to avoid log(0)

    // Step 1: Normalize the data for each indicator
    let mut normalized_data = Vec::with_capacity(num_indicators);
    for j in 0..num_indicators {
        let indicator_values = &data[j];

        // Find min and max for the current indicator
        // This is safe because num_samples > 0
        let mut min_val = indicator_values[0];
        let mut max_val = indicator_values[0];
        for i in 1..num_samples {
            let val = indicator_values[i];
            if val < min_val {
                min_val = val;
            }
            if val > max_val {
                max_val = val;
            }
        }

        let mut current_normalized_col = Vec::with_capacity(num_samples);
        let range = max_val - min_val;

        if range.abs() < epsilon {
            // All values in this column are (nearly) identical.
            // This indicator provides no discriminatory information.
            // Set normalized values to a constant (e.g., 1.0).
            // This leads to P_ij = 1/m for all i, then e_j = 1 (max entropy), d_j = 0.
            for _ in 0..num_samples {
                current_normalized_col.push(1.0);
            }
        } else {
            for &val in indicator_values {
                let norm_val = match types[j] {
                    IndicatorType::Positive => (val - min_val) / range,
                    IndicatorType::Negative => (max_val - val) / range,
                };
                // Clamp norm_val to [0, 1] to handle potential floating point inaccuracies.
                // Values should naturally be in [0,1] if data is clean.
                current_normalized_col.push(norm_val.max(0.0).min(1.0));
            }
        }
        normalized_data.push(current_normalized_col);
    }

    // Step 2: Calculate P_ij matrix (proportions)
    // P_ij = x'_ij / sum_k(x'_kj)
    let mut p_matrix = Vec::with_capacity(num_indicators);
    for j in 0..num_indicators {
        let norm_col = &normalized_data[j];
        let sum_norm_col: f64 = norm_col.iter().sum();

        let mut current_p_col = Vec::with_capacity(num_samples);
        if sum_norm_col.abs() < epsilon {
            // This case implies all normalized values were zero or cancelled out.
            // This should ideally be covered by the 'range.abs() < epsilon' check if all original values were identical.
            // If reached, treat as non-informative: P_ij = 1/m for all samples i.
            // This leads to maximum entropy e_j = 1, thus d_j = 0 for this indicator.
            let val_p = 1.0 / num_samples as f64;
            for _ in 0..num_samples {
                current_p_col.push(val_p);
            }
        } else {
            for &norm_val in norm_col {
                current_p_col.push(norm_val / sum_norm_col);
            }
        }
        p_matrix.push(current_p_col);
    }

    // Step 3: Calculate entropy e_j for each indicator
    // e_j = -k * sum(P_ij * ln(P_ij)), where k = 1 / ln(m)
    // num_samples > 1 is guaranteed here, so ln(num_samples) is valid.
    let k_entropy = 1.0 / (num_samples as f64).ln();
    let mut entropies = Vec::with_capacity(num_indicators);

    for j in 0..num_indicators {
        let p_col = &p_matrix[j];
        let mut e_j_sum_term = 0.0;
        for &p_ij in p_col {
            if p_ij > epsilon {
                // Check if p_ij is significantly greater than zero to avoid NaN from ln(0)
                // lim x->0+ (x * ln(x)) = 0.
                e_j_sum_term += p_ij * p_ij.ln();
            }
        }
        // Entropy e_j should be in [0,1]. Clamping for robustness.
        let e_j = (-k_entropy * e_j_sum_term).max(0.0).min(1.0);
        entropies.push(e_j);
    }

    // Step 4: Calculate divergence d_j (or information utility)
    // d_j = 1 - e_j
    // Since e_j is clamped to [0,1], d_j will also be in [0,1].
    let diversities: Vec<f64> = entropies.iter().map(|&e_j| 1.0 - e_j).collect();

    // Step 5: Calculate weights w_j
    // w_j = d_j / sum(d_k)
    let sum_diversities: f64 = diversities.iter().sum();

    if sum_diversities.abs() < epsilon {
        // All indicators have zero diversity (max entropy), implying they are equally uninformative
        // or provide no basis for differentiation according to this method. Assign equal weights.
        // num_indicators > 0 is guaranteed here.
        vec![1.0 / num_indicators as f64; num_indicators]
    } else {
        diversities
            .iter()
            .map(|&d_j| d_j / sum_diversities)
            .collect()
    }
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
                probability_norm(silhouette_scores.clone()),
                probability_norm(calinski_harabasz_scores.clone()),
                probability_norm(davies_bouldin_scores.clone()),
            ),
            "minmax" => (
                min_max_norm(silhouette_scores.clone()),
                min_max_norm(calinski_harabasz_scores.clone()),
                min_max_norm_rev(davies_bouldin_scores.clone()),
            ),
            "scale" => (
                scale_norm(silhouette_scores.clone()),
                scale_norm(calinski_harabasz_scores.clone()),
                scale_norm(davies_bouldin_scores.clone()),
            ),
            "zscore" => (
                z_score_norm(silhouette_scores.clone()),
                z_score_norm(calinski_harabasz_scores.clone()),
                z_score_norm(davies_bouldin_scores.clone()),
            ),
            _ => panic!("Unsupported normalization method: {}", arg.norm_method),
        };

    let n_values: Vec<u32> = scores.iter().map(|(n, _, _, _)| *n).collect();

    let s_devi = calculate_variance(silhouette_weights.clone());
    let c_devi = calculate_variance(calinski_harabasz_weights.clone());
    let d_devi = calculate_variance(davies_bouldin_weights.clone());
    let ve = probability_norm(vec![s_devi, c_devi, d_devi]);
    println!("The weights : {:?}", ve);

    let weights = entropy_weight_method(
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
