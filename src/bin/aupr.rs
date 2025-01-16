use clap::Parser;
use hound::{WavReader, WavWriter};
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    /// Input WAV file path
    input: PathBuf,

    /// Output WAV file path
    #[arg(default_value = "./output.wav")]
    output: PathBuf,

    /// Silence threshold in dB
    #[arg(short, long, default_value_t = -60.0)]
    threshold_db: f32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Open the input WAV file
    let reader = WavReader::open(cli.input).expect("Failed to open input file");
    let spec = reader.spec();
    let samples: Vec<i16> = reader.into_samples::<i16>().collect::<Result<_, _>>()?;

    // Calculate silence threshold based on dB
    let max_sample = i16::MAX as f32;
    let threshold = 10f32.powf(cli.threshold_db / 20.0) * max_sample;

    // Find non-silent regions
    let mut start_index = None;
    let mut end_index = None;
    let mut non_silent_regions = Vec::new();

    for (index, &sample) in samples.iter().enumerate() {
        if sample.abs() > threshold as i16 {
            if start_index.is_none() {
                start_index = Some(index);
            }
            end_index = Some(index);
        } else if start_index.is_some() && end_index.is_some() {
            non_silent_regions.push((start_index.unwrap(), end_index.unwrap()));
            start_index = None;
            end_index = None;
        }
    }

    if start_index.is_some() && end_index.is_some() {
        non_silent_regions.push((start_index.unwrap(), end_index.unwrap()));
    }

    // Write non-silent regions to the output WAV file
    let mut writer = WavWriter::create(
        cli.output,
        hound::WavSpec {
            channels: spec.channels,
            sample_rate: spec.sample_rate,
            bits_per_sample: spec.bits_per_sample,
            sample_format: hound::SampleFormat::Int,
        },
    )?;

    for &(start, end) in &non_silent_regions {
        for &sample in &samples[start..=end] {
            writer.write_sample(sample).expect("Failed to write sample");
        }
    }

    Ok(())
}



