use crows_wasm::{run_scenario, Runtime};
use std::sync::Arc;
use std::{collections::HashMap, time::Duration};
use tokio::sync::RwLock;
use tokio::time::sleep;
use uuid::Uuid;

use crows_utils::services::{
    connect_to_worker_to_coordinator, RunId, Worker, WorkerData, WorkerError,
    WorkerToCoordinatorClient,
};

type ScenariosList = Arc<RwLock<HashMap<String, Vec<u8>>>>;

#[derive(Clone)]
struct WorkerService {
    scenarios: ScenariosList,
    hostname: String,
    #[allow(dead_code)]
    environment: crows_wasm::Environment,
    client: WorkerToCoordinatorClient,
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

        let (runtime, mut info_handle) = Runtime::new(&scenario)
            .map_err(|err| WorkerError::CouldNotCreateRuntime(err.to_string()))?;

        run_scenario(runtime, config).await;

        let client = self.client.clone();
        tokio::spawn(async move {
            while let Some(info) = info_handle.receiver.recv().await {
                // TODO: technically we could queue the messages here and wait for the
                // worker to try to reconnect to the coordinator with a timeout
                // For now I prefer to just drop everything for simplicity as I'm not
                // concerned a lot about reconnections during a scenario run
                if let Err(_) = client.update(id.clone(), info).await {
                    // if the client send() returns an error it means the connection is broken
                    break;
                }
            }
        });

        Ok(())
    }

    async fn get_data(&self) -> WorkerData {
        WorkerData {
            id: Uuid::new_v4(),
            hostname: self.hostname.clone(),
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

    loop {
        let scenarios = scenarios.clone();
        let hostname = hostname.clone();

        println!("Connecting to {coordinator_address}");
        let create_service_callback = |client| async move {
            Ok(WorkerService {
                scenarios: scenarios.clone(),
                hostname,
                environment: crows_wasm::Environment::new()
                    .expect("Could not create a WASM environment"),
                client,
            })
        };
        let client = connect_to_worker_to_coordinator(coordinator_address.clone(), create_service_callback)
            .await
            .unwrap();

        loop {
            if let Err(_) = client.ping().await {
                // we lost connection, break and try to reconnect
                eprintln!("Connection to the coordinator is broken, waiting 5s for a reconnect try");
                sleep(Duration::from_secs(5)).await;
                break;
            }
            sleep(Duration::from_secs(1)).await;
        }
    }
}
