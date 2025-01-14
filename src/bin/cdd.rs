use clap::{ArgGroup, Parser};
use csv::ReaderBuilder;
use reqwest::Client;
use std::fs::{create_dir_all, File};
use std::path::PathBuf;
use tokio::fs::File as TokioFile;
use tokio::io::AsyncWriteExt;

#[derive(Parser)]
#[command(name = "csv_downloader")]
#[command(about = "A CLI tool to download URLs from a CSV file", long_about = None)]
#[command(group(
    ArgGroup::new("input")
        .required(true)
        .args(&["file", "url"])
))]
struct Args {
    /// Path to the CSV file
    #[arg(short, long)]
    file: Option<String>,

    /// Column name in the CSV that contains URLs
    #[arg(short, long)]
    column: String,

    /// Output directory for downloaded files
    #[arg(short, long, default_value = "./downloads")]
    output: PathBuf,

    /// URL to download directly (for testing purposes)
    #[arg(short, long)]
    url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();


    create_dir_all(&args.output).expect("Failed to create output directory");
    if let Some(url) = args.url {
        download_single_url(&url, &args.output).await?;
    } else if let Some(file_path) = args.file {
        download_urls_from_csv(&file_path, &args.column, &args.output).await?;
    }
    println!("Download complete.");

    Ok(())
}

async fn download_single_url(url: &str, output_dir: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let response = client.get(url).send().await?;
    let content = response.bytes().await?;

    let file_name = url.split('/').last().unwrap_or("downloaded_file");
    let file_path = format!("{}/{}", output_dir.display(), file_name);

    let mut file = TokioFile::create(&file_path).await?;
    file.write_all(&content).await?;

    println!("Downloaded {} to {}", url, file_path);
    Ok(())
}

async fn download_urls_from_csv(file_path: &str, column: &str, output_dir: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::open(file_path)?;
    let mut reader = ReaderBuilder::new().from_reader(file);
    let headers = reader.headers()?.clone();

    let url_column_index = headers.iter().position(|h| h == column)
        .ok_or_else(|| format!("Column '{}' not found in CSV", column))?;

    let urls: Vec<String> = reader.records()
        .filter_map(|r| r.ok())
        .filter_map(|r| r.get(url_column_index).map(String::from))
        .collect();

    let client = Client::new();

    let tasks: Vec<_> = urls.into_iter()
        .inspect(|url| {
            println!("Creating task for {}", &url);
        })
        .map(|url|
            download_single_url_with_client(&client, url, output_dir))
        .collect();

    futures::future::join_all(tasks).await;

    Ok(())
}

async fn download_single_url_with_client(client: &Client, url: String, output_dir: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let response = client.get(&url).send().await?;
    let content = response.bytes().await?;

    let file_name = url.split('/').last().unwrap_or("downloaded_file");
    let file_path = format!("{}/{}", output_dir.display(), file_name);

    println!("Downloading {} to {}", url, file_path);
    let mut file = Option::expect(TokioFile::create(&file_path).await.ok(), "Failed to create file");
    file.write_all(&content).await.expect("Failed to write to file");

    println!("Downloaded {} to {}", url, file_path);
    Ok(())
}