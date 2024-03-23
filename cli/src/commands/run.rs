use std::path::PathBuf;

use crows_cli::output::{drive_progress, LocalProgressFetcher};
use crows_utils::services::RunId;
use crows_wasm::{fetch_config, run_scenario};

pub async fn run(path: &PathBuf) -> anyhow::Result<()> {
    let scenario = std::fs::read(path).unwrap();
    let (runtime, info_handle) =
        crows_wasm::Runtime::new(&scenario).expect("Could not create a runtime");
    let (instance, _, mut store) = runtime
        .new_instance()
        .await
        .expect("Could not create an instance");
    let config = fetch_config(instance, &mut store)
        .await
        .expect("Config not found in the module");

    run_scenario(runtime, scenario, config).await;

    let mut client = LocalProgressFetcher::new(info_handle, "worker".to_string());

    drive_progress(&mut client, &RunId::new(), vec!["worker".to_string()])
        .await
        .expect("Error while running the scenario");

    Ok(())
}
