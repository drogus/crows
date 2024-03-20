use crows_shared::{Config, ConstantArrivalRateConfig};
use crows_wasm::Runtime;
use std::sync::Arc;
use std::time::Instant;
use std::{collections::HashMap, time::Duration};
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use uuid::Uuid;

use crows_utils::services::{
    connect_to_worker_to_coordinator, RunId, Worker, WorkerData, WorkerError,
    WorkerToCoordinatorClient,
};
use num_rational::Rational64;

type ScenariosList = Arc<RwLock<HashMap<String, Vec<u8>>>>;

// TODO: in the future we should probably share it with the coordinator, ie.
// coordinator should prepare the defaults based on the default module settings
// by examining the module
#[derive(Clone)]
struct RunInfo {
    run_id: RunId,
    concurrency: usize,
    rate: Rational64,
    module_name: String,
}

impl RunInfo {
    fn new(run_id: RunId, concurrency: usize, rate: Rational64, module_name: String) -> Self {
        Self {
            run_id,
            concurrency,
            rate,
            module_name,
        }
    }
}

#[derive(Clone)]
struct WorkerService {
    scenarios: ScenariosList,
    hostname: String,
    runs: HashMap<RunId, RunInfo>,
    environment: crows_wasm::Environment,
    client: Arc<Mutex<Option<WorkerToCoordinatorClient>>>,
}

impl Worker for WorkerService {
    async fn upload_scenario(&self, _: WorkerToCoordinatorClient, name: String, content: Vec<u8>) {
        self.scenarios.write().await.insert(name, content);
    }

    async fn ping(&self, _: WorkerToCoordinatorClient) -> String {
        todo!()
    }

    async fn start(&self, _: WorkerToCoordinatorClient, name: String, config: crows_shared::Config) -> Result<(), WorkerError> {
        let locked = self.scenarios.read().await;
        let scenario = locked
            .get(&name)
            .ok_or(WorkerError::ScenarioNotFound)?
            .clone();
        drop(locked);

        let runtime = Runtime::new(&scenario)
            .map_err(|err| WorkerError::CouldNotCreateRuntime(err.to_string()))?;
        let mut executor = Executors::create_executor(config, runtime).await;
        // TODO: prepare should be an entirely separate step and coordinator should wait for
        // prepare from all of the workers
        executor.prepare().await;
        executor.run().await;

        Ok(())
    }

    async fn get_data(&self, _: WorkerToCoordinatorClient) -> WorkerData {
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

    let wrapped_client: Arc<Mutex<Option<WorkerToCoordinatorClient>>> = Default::default();

    let service = WorkerService {
        scenarios: scenarios.clone(),
        hostname,
        runs: Default::default(),
        environment: crows_wasm::Environment::new().unwrap(),
        client: wrapped_client.clone(),
    };

    println!("Connecting to {coordinator_address}");
    let client = connect_to_worker_to_coordinator(coordinator_address, service)
        .await
        .unwrap();

    let mut locked = wrapped_client.lock().await;
    *locked = Some(client.clone());
    drop(locked);

    loop {
        // TODO: pinging should also work as an indicator of connection being alive
        client.ping().await?;
        sleep(Duration::from_secs(1)).await;
    }
}

trait Executor {
    async fn prepare(&mut self) -> anyhow::Result<()>;
    async fn run(&mut self) -> anyhow::Result<()>;
}

enum Executors {
    ConstantArrivalRateExecutor(ConstantArrivalRateExecutor),
}

impl Executors {
    pub async fn create_executor(config: Config, runtime: Runtime) -> Self {
        match config {
            Config::ConstantArrivalRate(config) => {
                Executors::ConstantArrivalRateExecutor(ConstantArrivalRateExecutor {
                    config,
                    runtime,
                })
            }
        }
    }

    pub async fn run(&mut self) {
        match self {
            Executors::ConstantArrivalRateExecutor(ref mut executor) => {
                executor.run().await.unwrap()
            }
        }
    }

    pub async fn prepare(&mut self) {
        match self {
            Executors::ConstantArrivalRateExecutor(ref mut executor) => {
                executor.prepare().await.unwrap()
            }
        }
    }
}

struct ConstantArrivalRateExecutor {
    config: ConstantArrivalRateConfig,
    runtime: Runtime,
}

// TODO: k6 supports an option to set maximum number of VUs. For now
// I haven't bothered to implement any limits, but it might be useful for bigger
// tests maybe?
impl Executor for ConstantArrivalRateExecutor {
    async fn run(&mut self) -> anyhow::Result<()> {
        let rate_per_second = self.config.rate as f64 / self.config.time_unit.as_secs_f64();
        let sleep_duration = Duration::from_secs_f64(1.0 / rate_per_second);

        let instant = Instant::now();
        loop {
            let handle = self.runtime.fetch_or_create_instance().await?;
            tokio::spawn(async move {
                if let Err(err) = handle.run_test().await {
                    eprintln!("An error occurred while running a scenario: {err:?}");
                }
            });
            // TODO: at the moment we always sleep for a calculated amount of time
            // This may be wrong, especially when duration is very low, because
            // with a very high request rate the time needed to spawn a task may
            // be substantial enough to delay execution. So technically we should
            // calculate how much time passed since sending the previous request and
            // only sleep for the remaining duration
            tokio::time::sleep(sleep_duration).await;

            // TODO: wait for all of the allocated instances finish, ie. implement
            // "graceful stop"
            if instant.elapsed() > self.config.duration {
                return Ok(());
            }
        }
    }

    async fn prepare(&mut self) -> anyhow::Result<()> {
        let vus = self.config.allocated_vus;
        for _ in 0..vus {
            self.runtime.reserve_instance().await?;
        }

        Ok(())
    }
}
