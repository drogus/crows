use crows_wasm::{InfoHandle, Runtime, InfoMessage};
use executors::Executors;
use std::sync::Arc;
use std::{collections::HashMap, time::Duration};
use tokio::sync::RwLock;
use tokio::time::sleep;
use uuid::Uuid;

use crows_utils::services::{
    connect_to_worker_to_coordinator, RunId, Worker, WorkerData, WorkerError,
    WorkerToCoordinatorClient, RunInfo,
};

mod executors;

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
    async fn upload_scenario(&self, _: WorkerToCoordinatorClient, name: String, content: Vec<u8>) {
        self.scenarios.write().await.insert(name, content);
    }

    async fn ping(&self, _: WorkerToCoordinatorClient) -> String {
        todo!()
    }

    async fn start(
        &self,
        _: WorkerToCoordinatorClient,
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

        let mut executor = Executors::create_executor(config, runtime).await;

        tokio::spawn(async move {
            // TODO: prepare should be an entirely separate step and coordinator should wait for
            // prepare from all of the workers
            executor.prepare().await;
            executor.run().await;
        });

        self.runs.write().await.insert(id, info_handle);

        Ok(())
    }

    async fn get_data(&self, _: WorkerToCoordinatorClient) -> WorkerData {
        WorkerData {
            id: Uuid::new_v4(),
            hostname: self.hostname.clone(),
        }
    }

    async fn get_run_status(&self, _: WorkerToCoordinatorClient, id: RunId) -> RunInfo {
        let mut run_info: RunInfo = Default::default();
        run_info.done = false;

        if let Some(handle) = self.runs.write().await.get_mut(&id) {
            while let Ok(update) = handle.receiver.try_recv() {
                match update {
                    InfoMessage::Stderr(buf) => run_info.stderr.push(buf),
                    InfoMessage::Stdout(buf) => run_info.stdout.push(buf),
                    InfoMessage::RequestInfo(info) => run_info.request_stats.push(info),
                    InfoMessage::IterationInfo(info) => run_info.iteration_stats.push(info),
                    InfoMessage::InstanceCheckedOut => run_info.active_instances_delta += 1,
                    InfoMessage::InstanceReserved => run_info.capacity_delta += 1,
                    InfoMessage::InstanceCheckedIn => run_info.active_instances_delta -= 1,
                    InfoMessage::TimingUpdate((elapsed, left)) => {
                        run_info.elapsed = Some(elapsed);
                        run_info.left = Some(left);
                    },
                    crows_wasm::InfoMessage::Done => run_info.done = true,
                }
            }
        }

        run_info
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

    let service = WorkerService {
        scenarios: scenarios.clone(),
        hostname,
        runs: Default::default(),
        environment: crows_wasm::Environment::new().unwrap(),
    };

    println!("Connecting to {coordinator_address}");
    let client = connect_to_worker_to_coordinator(coordinator_address, service)
        .await
        .unwrap();

    loop {
        // TODO: pinging should also work as an indicator of connection being alive
        client.ping().await?;
        sleep(Duration::from_secs(1)).await;
    }
}
