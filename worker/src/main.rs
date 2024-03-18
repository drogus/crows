#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

use crows_shared::ConstantArrivalRateConfig;
use crows_wasm::Runtime;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use std::{collections::HashMap, env::args_os, time::Duration};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, timeout};
use tokio::prelude::*;
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

enum RuntimeMessage {}

#[derive(Clone)]
struct WorkerService {
    scenarios: ScenariosList,
    hostname: String,
    wasm_actor: UnboundedSender<RuntimeMessage>,
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

        let (mut instance, _) = crows_wasm::Instance::new(&scenario, &self.environment).await.map_err(|err| WorkerError::CouldNotCreateModule)?;

        // spawn modules

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

    let coordinator_address: String = std::env::var("COORDINATOR_ADDRESS").unwrap_or("127.0.0.1:8181".into());
    let hostname: String = std::env::var("WORKER_NAME").unwrap();

    println!("Starting with hostname: {hostname}");
    // let handles: Vec<RuntimeHandle> = Default::default();
    let scenarios: ScenariosList = Default::default();
    let (wasm_sender, wasm_receiver) = unbounded_channel();

    let service = WorkerService {
        scenarios: scenarios.clone(),
        hostname,
        wasm_actor: wasm_sender,
        runs: Default::default(),
        environment: crows_wasm::Environment::new().unwrap()
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


trait Executor<'a> {
    async fn run(&mut self, runtime: Runtime<'a>, config: ConstantArrivalRateConfig) -> anyhow::Result<()>;
}

struct ConstantArrivalRateExecutor {
    config: ConstantArrivalRateConfig
}

impl<'a> Executor<'a> for ConstantArrivalRateExecutor {
    async fn run(&mut self, runtime: Runtime<'a>, config: ConstantArrivalRateConfig) -> anyhow::Result<()> {
        println!("Start");
        let instant = Instant::now();
        let future = async move {
            loop {
                println!("elapsed: {}ms", instant.elapsed().as_millis());
                tokio::time::sleep(Duration::from_secs(1));
            }
        };

        tokio::timer::Timeout::new(future, config.duration).await;

        Ok(())
    }
}
