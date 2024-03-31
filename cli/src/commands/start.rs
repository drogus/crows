use crows::output::drive_progress;
use crows_utils::{services::CoordinatorClient, InfoMessage};
use tokio::sync::mpsc::UnboundedReceiver;

pub async fn start(
    coordinator: &mut CoordinatorClient,
    name: &str,
    workers_number: &usize,
    updates_receiver: UnboundedReceiver<(String, InfoMessage)>
) -> anyhow::Result<()> {
    let (_, mut worker_names) = coordinator
        .start(name.to_string(), workers_number.clone())
        .await
        .unwrap()
        .unwrap();

    worker_names.sort();

    drive_progress(worker_names, updates_receiver)
        .await
        .expect("Error while running a scenario");

    Ok(())
}
