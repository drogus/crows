#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

use std::{collections::HashMap, env::args_os, time::Duration};
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use uuid::Uuid;

use crows_utils::services::{connect_to_worker_to_coordinator, Worker, WorkerData, WorkerError};

#[derive(Serialize, Deserialize, Clone)]
struct WorkerService {
    scenarios: HashMap<String, Vec<u8>>,
    hostname: String,
}

impl Worker for WorkerService {
    async fn upload_scenario(&mut self, name: String, content: Vec<u8>) {
        todo!()
        // self.scenarios.insert(name, content);
    }

    async fn ping(&self) -> String {
        todo!()
    }

    async fn start(&self, name: String, concurrency: usize) -> Result<(), WorkerError> {
        todo!()
        // let scenario = self
        //     .scenarios
        //     .get(&name)
        //     .ok_or(WorkerError::ScenarioNotFound)?
        //     .clone();
        //
        // let args = (scenario, name, concurrency);
        // let _process = spawn!(|args, mailbox: Mailbox<()>| {
        //     let (scenario, name, concurrency) = args;
        //
        //     println!("Running {name} scenario with {concurrency} concurrency.");
        //
        //     let module = WasmModule::new(&scenario).unwrap();
        //     let monitorable = mailbox.monitorable();
        //     let mut processes = Vec::new();
        //     for _ in 0..concurrency {
        //         match module.spawn::<(), Bincode>("_start", &[]) {
        //             Ok(process) => {
        //                 processes.push(process.id());
        //                 monitorable.monitor(process);
        //             }
        //             Err(e) => {
        //                 println!("Could not start process {name}: {e:?}");
        //             }
        //         }
        //     }
        //
        //     loop {
        //         match monitorable.receive() {
        //             MessageSignal::Signal(ProcessDiedSignal(id)) => {
        //                 if let Some(index) = processes.iter().position(|x| *x == id) {
        //                     processes.remove(index);
        //                     if processes.is_empty() {
        //                         break;
        //                     }
        //                 }
        //             }
        //             MessageSignal::Message(_) => {}
        //         }
        //     }
        // });
        //
        // Ok(())
    }

    async fn get_data(&self) -> WorkerData {
        WorkerData {
            id: Uuid::new_v4(),
            hostname: self.hostname.clone(),
        }
    }
}

#[tokio::main]
async fn main() {
    let coordinator_address: String = std::env::var("COORDINATOR_ADDRESS")
        .unwrap();
    let hostname: String = std::env::var("WORKER_NAME")
        .unwrap();


    println!("Starting with hostname: {hostname}");
    let scenarios: HashMap<String, Vec<u8>> = Default::default();
    let service = WorkerService {
        scenarios,
        hostname,
    };

            println!("Connecting to {coordinator_address}");
            let mut client =
                connect_to_worker_to_coordinator(coordinator_address, service).await.unwrap();

            loop {
                client.ping();
                sleep(Duration::from_secs(1));
            }
}
