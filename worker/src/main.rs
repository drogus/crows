use crows_shared::{Config, ConstantArrivalRateConfig};
use crows_wasm::Runtime;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use std::{collections::HashMap, env::args_os, time::Duration};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, timeout, Timeout};
use uuid::Uuid;

use crows_utils::services::{
    connect_to_worker_to_coordinator, RunId, Worker, WorkerData, WorkerError,
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
}

impl Worker for WorkerService {
    async fn upload_scenario(&mut self, name: String, content: Vec<u8>) {
        self.scenarios.write().await.insert(name, content);
    }

    async fn ping(&self) -> String {
        todo!()
    }

    // async fn prepare(
    //     &mut self,
    //     id: ModuleId,
    //     concurrency: usize,
    //     rate: Rational64,
    // ) -> Result<RunId, WorkerError> {
    //     let run_id: RunId = RunId::new();
    //
    //     // TODO: we should check if we have a given module available and if not ask coordinator
    //     // to send it. For now let's assume we have the module id
    //     let info = RunInfo::new(run_id.clone(), concurrency, rate, id);
    //     self.runs.insert(run_id.clone(), info);
    //
    //     Ok(run_id)
    // }

    async fn start(&self, name: String, config: crows_shared::Config) -> Result<(), WorkerError> {
        // PLAN
        // either pass as an argument or fetch Executor::Config?
        let locked = self.scenarios.read().await;
        let scenario = locked
            .get(&name)
            .ok_or(WorkerError::ScenarioNotFound)?
            .clone();
        drop(locked);

        // TODO: remove unwrap
        let runtime = Runtime::new(&scenario).unwrap();
        let mut executor = Executors::get_executor(config, runtime).await;
        // TODO: prepare should be an entirely separate step and coordinator should wait for
        // prepare from all of the workers
        executor.prepare().await;
        executor.run().await;

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
    // TODO: allow to set the number of CPUs
    let cpus = num_cpus::get();

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
    let mut client = connect_to_worker_to_coordinator(coordinator_address, service)
        .await
        .unwrap();

    loop {
        // TODO: pinging should also work as an indicator of connection being alive
        client.ping();
        sleep(Duration::from_secs(1));
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
    pub async fn get_executor(config: Config, runtime: Runtime) -> Self {
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
            tokio::time::sleep(sleep_duration).await;

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
