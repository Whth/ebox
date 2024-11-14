use std::env;
use async_openai::types::{ChatCompletionRequestUserMessageArgs};
use async_openai::{types::CreateChatCompletionRequestArgs, Client};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, read_to_string, write};
use std::io::{stdout, Write};
use std::path::PathBuf;
use async_openai::config::OpenAIConfig;
use futures::stream::StreamExt;
use shellexpand::tilde;
use prettytable::{row,Table};
use  std::process::Command as Process;
use chrono::{DateTime, Utc};

#[derive(Parser)]
#[command(name = "openai-cli")]
#[command(about = "A command-line interface for OpenAI API")]
struct Cli {
    /// The API key to use
    #[clap(short, long, env = "OPENAI_API_KEY")]
    key: Option<String>,

    /// The host of the OpenAI API
    #[clap( long, default_value = "https://api.openai.com/v1", env = "OPENAI_API_HOST")]
    host_url: Option<String>,


    #[clap(short, long,env = "OPENAI_MODEL")]
    /// The model to use
    model: Option<String>,


    /// The path to the config file
    #[clap(short, long, default_value = "~/.config/openai-cli/config.toml", env = "OPENAI_CONFIG_FILE")]
    config_file: String,

    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    #[command(visible_alias = "c",about = "Chat with the OpenAI API")]
    Chat {
        /// The prompt to send to the OpenAI API
        prompt: Vec<String>,

        /// Maximum number of tokens to generate in the completion.
        #[arg(short,long, default_value_t = 1000)]
        max_tokens: u32,

        /// What sampling temperature to use, between 0 and 2. Higher values like 0.8 will make the output more random, while lower values like 0.2 will make it more focused and deterministic. (Optional)
        #[arg(short,long, default_value_t = 0.7)]
        temperature: f32,


        /// An alternative to sampling with temperature, called nucleus sampling, where the model considers the results of the tokens with top_p probability mass. So 0.1 means only the tokens comprising the top 10% probability mass are considered. (Optional)
        #[arg(short,long, default_value_t = 1.0)]
        top_p:f32
    },
    #[command(visible_alias = "l",about = "List all available models")]
    Models,

    #[command(visible_alias = "p",about = "Pin a specific model")]
    Pin{
        /// The name of the model to pin
        #[arg(value_name = "MODEL")]
        model: String,
    }
}

#[derive(Serialize, Deserialize,  Default)]
struct Config {
    /// API key for accessing the service. (Optional)
    /// Example: "your_api_key_here"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// Host URL of the API service. (Optional)
    /// Example: "https://api.example.com"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_host: Option<String>,

    /// Model to be used. (Optional)
    /// Example: "model_v1"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,


}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config_path = tilde(&cli.config_file).to_string();

    let mut config: Config = if let Ok(content) = read_to_string(config_path.clone()) {
        toml::from_str(&content)?
    } else {
        let conf= Config::default();
        create_dir_all(PathBuf::from(&config_path).parent().expect("Failed to get parent directory of config file")).expect("Failed to create config directory");
        write(&config_path, toml::to_string(&conf).expect("Failed to serialize default config")).expect("Failed to write default config");
        conf


    };


    let client = Client::with_config(OpenAIConfig::default()
        .with_api_key(cli.key.or(config.api_key.take()).ok_or("API key is required")?)
        .with_api_base(cli.host_url.or(config.api_host.take()).ok_or("API host is required")?)
        );

    match &cli.command {
        Commands::Chat { prompt, max_tokens, temperature, top_p} => {
            let request = CreateChatCompletionRequestArgs::default()
                .model(cli.model.or(config.model.take()).ok_or("Model is required")?.as_str())
                .max_tokens(*max_tokens)
                .temperature(*temperature)
                .top_p(*top_p)
                .messages([ChatCompletionRequestUserMessageArgs::default()
                    .content(prompt.join(" ").as_str())
                    .build()
                    .expect("Failed to build user message")
                    .into()])
                .build()
                .expect("Failed to build request");

            let mut stream = client.chat().create_stream(request).await.expect("Failed to create stream");

            let mut lock = stdout().lock();
            while let Some(result) = stream.next().await {
                match result {
                    Ok(response) => {
                        response.choices.iter().for_each(|chat_choice| {
                            if let Some(ref content) = chat_choice.delta.content {
                                write!(lock, "{}", content).unwrap();
                            }
                        });
                    }
                    Err(err) => {
                        writeln!(lock, "error: {err}").unwrap();
                    }
                }
                stdout().flush()?;
            }
        }
        Commands::Models => {
            let mut models = client.models().list().await.expect("Failed to list models");


            // 按创建时间排序

            // 创建表格
            let mut table = Table::default();
            table.add_row(row!["Index", "ID", "Owned By", "Created"]);

            models.data.sort_by_key(|m| -(m.created as i32));
            models.data.iter().enumerate()
                .for_each(
                |(ind, model)|{
                    let created_time: DateTime<Utc> = DateTime::from_timestamp(model.created as i64, 0).expect("Failed to parse timestamp");
                    table.add_row(row![ind, model.id, model.owned_by, created_time.format("%Y-%m-%d")]);
                }
                );

            // 打印表格
            table.printstd();


        }

        Commands::Pin {
            model
        }=> {

            let model_name=if model.chars().all(|c| c.is_ascii_digit()){
                let index: usize = model.parse().expect("Invalid index");
                let mut models=client.models().list().await.expect("Failed to list models").data;
                models.sort_by_key(|m| -(m.created as i32));
                models.get(index).expect("Invalid index").id.clone()

            }else { model.clone() };



            env::set_var("OPENAI_MODEL", &model_name);
            Process::new("setx")
                .arg("OPENAI_MODEL")
                .arg(&model_name)
                .spawn()
                .expect("Failed to execute command");
            println!("Pinned model: {}", model_name);
        }
    }

    Ok(())
}