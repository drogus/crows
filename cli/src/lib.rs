pub mod output;
pub mod commands;

pub use commands::run;

use crows_utils::{services::{Client, CoordinatorClient, RunId, connect_to_coordinator}, InfoMessage};
use tokio::sync::mpsc::{UnboundedSender, UnboundedReceiver, unbounded_channel};

#[derive(Clone)]
pub struct ClientService {
    pub updates_sender: UnboundedSender<(String, InfoMessage)>,
}

impl Client for ClientService {
    async fn update(&self, _: RunId, worker_name: String, info: InfoMessage) {
        let _= self.updates_sender.send((worker_name, info));
    }
}

pub async fn create_coordinator() -> anyhow::Result<(CoordinatorClient, UnboundedReceiver<(String, InfoMessage)>)>
{
    let (updates_sender, updates_receiver) = unbounded_channel();
    let url = std::env::var("CROWS_COORDINATOR_URL").unwrap_or("127.0.0.1:8282".to_string());
    let coordinator =
        connect_to_coordinator(url, |_| async { Ok(ClientService { updates_sender }) }).await?;
    Ok((coordinator, updates_receiver))
}
