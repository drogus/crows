use std::path::PathBuf;

use crows_utils::services::connect_to_coordinator;
use crows_utils::services::Client;

use clap::{Parser, Subcommand};
use crows_wasm::fetch_config;
use crows_wasm::run_scenario;

mod commands;

#[derive(Clone)]
struct ClientService;

impl Client for ClientService {}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Uploads a scenario
    Upload {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        path: PathBuf,
    },
    Start {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        workers_number: usize,
    },
    Run {
        #[arg()]
        path: PathBuf,
    },
    Workers {
        #[command(subcommand)]
        command: Option<WorkersCommands>,
    },
}

#[derive(Subcommand)]
enum WorkersCommands {
    /// List available workers
    List,
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let service = ClientService {};
    let mut coordinator = connect_to_coordinator("127.0.0.1:8282", service)
        .await
        .unwrap();

    match &cli.command {
        Some(Commands::Upload { name, path }) => {
            let content = std::fs::read(path).unwrap();
            coordinator
                .upload_scenario(name.clone(), content)
                .await
                .unwrap()
                .unwrap();
        }
        Some(Commands::Start {
            name,
            workers_number,
        }) => {
            commands::start(&mut coordinator, name, workers_number).await?;
        }
        Some(Commands::Workers { command }) => match &command {
            Some(WorkersCommands::List) => {
                let workers = coordinator.list_workers().await.unwrap();
                println!(
                    "Available workers list:\n{}",
                    workers
                        .iter()
                        .map(|w| format!("\t{w}\n"))
                        .collect::<Vec<String>>()
                        .join("")
                );
            }
            None => {}
        },
        Some(Commands::Run { path }) => {
            commands::run(path).await.expect("An error while running a scenario");
        },
        None => {}
    }

    Ok(())
}
