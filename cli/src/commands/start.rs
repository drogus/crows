use crows_cli::output::drive_progress;
use crows_utils::services::CoordinatorClient;

pub async fn start(
    coordinator: &mut CoordinatorClient,
    name: &str,
    workers_number: &usize,
) -> anyhow::Result<()> {
    let (run_id, mut worker_names) = coordinator
        .start(name.to_string(), workers_number.clone())
        .await
        .unwrap()
        .unwrap();

    worker_names.sort();

    drive_progress(coordinator, &run_id, worker_names)
        .await
        .expect("Error while running a scenario");

    Ok(())
}
