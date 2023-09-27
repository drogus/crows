#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

use std::time::Duration;

use tokio::time::sleep;
use utils::services::connect_to_coordinator;
use utils::services::{Client, Coordinator};

use std::path::PathBuf;

use clap::{Parser, Subcommand};

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
        concurrency: usize,
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
pub async fn main() {
    let cli = Cli::parse();
    let service = ClientService {};
    let coordinator = connect_to_coordinator("127.0.0.1:8282", service)
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
        Some(Commands::Start { name, concurrency }) => {
            coordinator
                .start(name.to_string(), concurrency.clone())
                .await
                .unwrap();
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
        None => {}
    }
}
