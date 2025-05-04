use std::collections::HashMap;

use crate::output::drive_progress;
use crows_utils::{services::CoordinatorClient, InfoMessage};
use tokio::sync::mpsc::UnboundedReceiver;

pub async fn start(
    coordinator: &mut CoordinatorClient,
    name: &str,
    workers_number: &usize,
    env_vars: HashMap<String, String>,
    updates_receiver: UnboundedReceiver<(String, InfoMessage)>,
) -> anyhow::Result<()> {
    let (_, mut worker_names) = coordinator
        .start(name.to_string(), workers_number.clone(), env_vars)
        .await
        .unwrap()
        .unwrap();

    worker_names.sort();

    drive_progress(worker_names, updates_receiver)
        .await
        .expect("Error while running a scenario");

    Ok(())
}
