#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

use crows_wasm::Runtime;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{collections::HashMap, env::args_os, time::Duration};
use tokio::sync::{RwLock, Mutex};
use tokio::time::sleep;
use uuid::Uuid;

use crows_utils::services::{connect_to_worker_to_coordinator, Worker, WorkerData, WorkerError, RunId};
use crows_utils::ModuleId;
use num_rational::Rational64;

type ScenariosList = Arc<RwLock<HashMap<ModuleId, Vec<u8>>>>;

// TODO: in the future we should probably share it with the coordinator, ie.
// coordinator should prepare the defaults based on the default module settings
// by examining the module
#[derive(Clone)]
struct RunInfo {
    run_id: RunId,
    concurrency: usize,
    rate: Rational64,
    module_id: ModuleId,
}

impl RunInfo {
    fn new(run_id: RunId, concurrency: usize, rate: Rational64, module_id: ModuleId) -> Self {
        Self { run_id, concurrency, rate, module_id }
    }
}

#[derive(Clone)]
struct WorkerService<'a> {
    scenarios: ScenariosList,
    hostname: String,
    wasm_handles: Arc<Mutex<Vec<RuntimeHandle<'a>>>>,
    runs: HashMap<RunId, RunInfo>
}

impl<'a> Worker for WorkerService<'a> {
    async fn upload_scenario(&mut self, id: ModuleId, content: Vec<u8>) {
        self.scenarios.write().await.insert(id, content);
    }

    async fn ping(&self) -> String {
        todo!()
    }

    async fn prepare(&mut self, id: ModuleId, concurrency: usize, rate: Rational64) -> Result<RunId, WorkerError> {
        let run_id: RunId = RunId::new();

        // TODO: we should check if we have a given module available and if not ask coordinator
        // to send it. For now let's assume we have the module id
        let info = RunInfo::new(run_id.clone(), concurrency, rate, id);
        self.runs.insert(run_id.clone(), info);

        Ok(run_id)
    }

    async fn start(&self, id: ModuleId, concurrency: usize) -> Result<(), WorkerError> {
        let locked = self
            .scenarios
            .read()
            .await;
        let scenario = locked
            .get(&id)
            .ok_or(WorkerError::ScenarioNotFound)?
            .clone();
        drop(locked);

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

#[derive(Clone)]
struct RuntimeHandle<'a> {
    runtime: Arc<Mutex<Runtime<'a>>>,
}

impl<'a> RuntimeHandle<'a> {
    pub fn new(runtime: Runtime<'a>) -> Self {
        Self { runtime: Arc::new(Mutex::new(runtime)) }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO: allow to set the number of CPUs
    let cpus = num_cpus::get();

    let coordinator_address: String = std::env::var("COORDINATOR_ADDRESS").unwrap();
    let hostname: String = std::env::var("WORKER_NAME").unwrap();

    println!("Starting with hostname: {hostname}");
    let handles: Arc<Mutex<Vec<RuntimeHandle>>> = Default::default();
    let scenarios: ScenariosList = Default::default();
    let service = WorkerService {
        scenarios: scenarios.clone(),
        hostname,
        wasm_handles: handles.clone(),
        runs: Default::default()
    };

    std::thread::scope(|s| {
        let mut threads = Vec::new();

        let thread = s.spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            rt.spawn(async move {
                println!("Connecting to {coordinator_address}");
                let mut client = connect_to_worker_to_coordinator(coordinator_address, service)
                    .await
                    .unwrap();

                loop {
                    // TODO: pinging should also work as an indicator of connection being alive
                    client.ping();
                    sleep(Duration::from_secs(1));
                }
            });
        });

        threads.push(thread);

        for _ in (0..cpus).into_iter() {
            let scenarios = scenarios.clone();
            let handles = handles.clone();
            let thread = s.spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                let wasm_runtime = RuntimeHandle::new(Runtime::new().expect("Couldn't create a Runtime"));

                rt.spawn(async move {
                    handles.lock().await.push(wasm_runtime.clone());
                });
            });
            threads.push(thread);
        }

        for thread in threads {
            thread.join();
        }
    });

    Ok(())
}
