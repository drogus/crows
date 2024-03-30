use crows_utils::{process_info_handle, InfoHandle};
use crows_wasm::{run_scenario, Runtime};
use std::sync::Arc;
use std::{collections::HashMap, time::Duration};
use tokio::sync::RwLock;
use tokio::time::sleep;
use uuid::Uuid;

use crows_utils::services::{
    connect_to_worker_to_coordinator, RunId, RunInfo, Worker, WorkerData, WorkerError,
    WorkerToCoordinatorClient,
};

type ScenariosList = Arc<RwLock<HashMap<String, Vec<u8>>>>;
type RunsList = Arc<RwLock<HashMap<RunId, InfoHandle>>>;

#[derive(Clone)]
struct WorkerService {
    scenarios: ScenariosList,
    hostname: String,
    runs: RunsList,
    environment: crows_wasm::Environment,
}

impl Worker for WorkerService {
    async fn upload_scenario(&self, name: String, content: Vec<u8>) {
        self.scenarios.write().await.insert(name, content);
    }

    async fn ping(&self) -> String {
        todo!()
    }

    async fn start(
        &self,
        name: String,
        config: crows_shared::Config,
        id: RunId,
    ) -> Result<(), WorkerError> {
        let locked = self.scenarios.read().await;
        let scenario = locked
            .get(&name)
            .ok_or(WorkerError::ScenarioNotFound)?
            .clone();
        drop(locked);

        let (runtime, info_handle) = Runtime::new(&scenario)
            .map_err(|err| WorkerError::CouldNotCreateRuntime(err.to_string()))?;

        run_scenario(runtime, scenario, config).await;

        self.runs.write().await.insert(id, info_handle);

        Ok(())
    }

    async fn get_data(&self) -> WorkerData {
        WorkerData {
            id: Uuid::new_v4(),
            hostname: self.hostname.clone(),
        }
    }

    async fn get_run_status(&self, id: RunId) -> RunInfo {
        if let Some(handle) = self.runs.write().await.get_mut(&id) {
            return process_info_handle(handle).await;
        } else {
            Default::default()
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let coordinator_address: String =
        std::env::var("COORDINATOR_ADDRESS").unwrap_or("127.0.0.1:8181".into());
    let hostname: String = std::env::var("WORKER_NAME").unwrap();

    println!("Starting with hostname: {hostname}");
    // let handles: Vec<RuntimeHandle> = Default::default();
    let scenarios: ScenariosList = Default::default();

    println!("Connecting to {coordinator_address}");
    let create_service_callback = |_client| async move {
        Ok(WorkerService {
            scenarios: scenarios.clone(),
            hostname,
            runs: Default::default(),
            environment: crows_wasm::Environment::new().expect("Could not create a WASM environment"),
        })
    };
    let client = connect_to_worker_to_coordinator(coordinator_address, create_service_callback)
        .await
        .unwrap();

    loop {
        // TODO: pinging should also work as an indicator of connection being alive
        client.ping().await?;
        sleep(Duration::from_secs(1)).await;
    }
}
