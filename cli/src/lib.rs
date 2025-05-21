pub mod commands;
pub mod output;
pub mod printers;

pub use commands::run;

use crows_utils::{
    services::{connect_to_coordinator, Client, CoordinatorClient, RunId},
    InfoMessage,
};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Clone)]
pub struct ClientService {
    pub updates_sender: UnboundedSender<(String, InfoMessage)>,
}

impl Client for ClientService {
    async fn update(&self, _: RunId, worker_name: String, info: InfoMessage) {
        let _ = self.updates_sender.send((worker_name, info));
    }
}

pub async fn create_coordinator(
) -> anyhow::Result<(CoordinatorClient, UnboundedReceiver<(String, InfoMessage)>)> {
    let (updates_sender, updates_receiver) = unbounded_channel();
    let url = std::env::var("CROWS_COORDINATOR_URL").unwrap_or("127.0.0.1:8282".to_string());
    let coordinator =
        connect_to_coordinator(url, |_| async { Ok(ClientService { updates_sender }) }).await?;
    Ok((coordinator, updates_receiver))
}
