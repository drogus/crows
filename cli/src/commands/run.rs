use std::path::PathBuf;

use crows::output::drive_progress;
use crows_wasm::{fetch_config, run_scenario};
use tokio::sync::mpsc::unbounded_channel;

pub async fn run(path: &PathBuf) -> anyhow::Result<()> {
    let scenario = std::fs::read(path).unwrap();
    let (runtime, mut info_handle) =
        crows_wasm::Runtime::new(&scenario).expect("Could not create a runtime");
    let (instance, _, mut store) = runtime
        .new_instance()
        .await
        .expect("Could not create an instance");
    let config = fetch_config(instance, &mut store)
        .await
        .expect("Config not found in the module");

    run_scenario(runtime, config).await;

    let (updates_sender, updates_receiver) = unbounded_channel();

    tokio::spawn(async move {
        while let Some(info) = info_handle.receiver.recv().await {
            if let Err(_) = updates_sender.send(("worker".into(), info)) {
                break;
            }
        }
    });

    drive_progress(vec!["worker".to_string()], updates_receiver)
        .await
        .expect("Error while running the scenario");

    Ok(())
}
