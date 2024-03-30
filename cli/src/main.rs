use std::path::PathBuf;

use crows_utils::services::connect_to_coordinator;
use crows_utils::services::{Client, CoordinatorClient};

use clap::{Parser, Subcommand};

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

async fn create_coordinator() -> anyhow::Result<CoordinatorClient> {
    let url = std::env::var("CROWS_COORDINATOR_URL").unwrap_or("127.0.0.1:8282".to_string());
    Ok(connect_to_coordinator(url, |_| async { Ok(ClientService {}) }).await?)
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Upload { name, path }) => {
            let content = std::fs::read(path)?;
            let coordinator = create_coordinator().await?;

            coordinator.upload_scenario(name.clone(), content).await??;
        }
        Some(Commands::Start {
            name,
            workers_number,
        }) => {
            let mut coordinator = create_coordinator().await?;
            commands::start(&mut coordinator, name, workers_number).await?;
        }
        Some(Commands::Workers { command }) => match &command {
            Some(WorkersCommands::List) => {
                let coordinator = create_coordinator().await?;
                let workers = coordinator.list_workers().await?;
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
            commands::run(path)
                .await
                .expect("An error while running a scenario");
        }
        None => {}
    }

    Ok(())
}
