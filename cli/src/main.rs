use std::collections::HashMap;
use std::path::PathBuf;

use crows_utils::services::{connect_to_coordinator, RunId};
use crows_utils::services::{Client, CoordinatorClient};
use crows_utils::InfoMessage;

use clap::{Parser, Subcommand};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub use crows::commands;
pub use crows::output;

#[derive(Clone)]
struct ClientService {
    updates_sender: UnboundedSender<(String, InfoMessage)>,
}

impl Client for ClientService {
    async fn update(&self, _: RunId, worker_name: String, info: InfoMessage) {
        let _= self.updates_sender.send((worker_name, info));
    }
}

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
        #[arg(short, long)]
        env: Vec<String>,
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

async fn create_coordinator() -> anyhow::Result<(CoordinatorClient, UnboundedReceiver<(String, InfoMessage)>)>
{
    let (updates_sender, updates_receiver) = unbounded_channel();
    let url = std::env::var("CROWS_COORDINATOR_URL").unwrap_or("127.0.0.1:8282".to_string());
    let coordinator =
        connect_to_coordinator(url, |_| async { Ok(ClientService { updates_sender }) }).await?;
    Ok((coordinator, updates_receiver))
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Upload { name, path }) => {
            let content = std::fs::read(path)?;
            let (coordinator, _) = create_coordinator().await?;

            coordinator.upload_scenario(name.clone(), content).await??;
        }
        Some(Commands::Start {
            name,
            workers_number,
            env,
        }) => {
            let (mut coordinator, updates_receiver) = create_coordinator().await?;
            let env_vars: HashMap<String, String> = env.into_iter()
                .filter_map(|s| {
                    let parts: Vec<&str> = s.splitn(2, '=').collect();
                    if parts.len() == 2 {
                        Some((parts[0].to_string(), parts[1].to_string()))
                    } else {
                        None
                    }
                })
                .collect();
            commands::start(&mut coordinator, name, workers_number, env_vars, updates_receiver).await?;
        }
        Some(Commands::Workers { command }) => match &command {
            Some(WorkersCommands::List) => {
                let (coordinator, _) = create_coordinator().await?;
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
