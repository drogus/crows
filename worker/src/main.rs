#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

use std::collections::HashMap;
use std::time::Duration;

use tokio::time::sleep;
use utils::services::{connect_to_worker_to_coordinator, WorkerError};
use utils::services::{Worker, WorkerToCoordinator};
use uuid::Uuid;

struct Scenario {
    name: String,
    content: Vec<u8>,
}

#[derive(Default)]
struct WorkerService {
    scenarios: HashMap<String, Scenario>
}

impl Worker for WorkerService {
    async fn upload_scenario(&mut self, name: String, content: Vec<u8>) -> Result<(), WorkerError> {
        let scenario = Scenario { name: name.clone(), content };
        self.scenarios.insert(name, scenario);

        Ok(())
    }

    async fn ping(&self) -> String {
        "pong".to_string()
    }

    async fn start(&self, name: String) {
        println!("Sarting scenario {name}");
        
    }
}

#[tokio::main]
pub async fn main() {
    let service = WorkerService::default();
    let coordinator = connect_to_worker_to_coordinator("127.0.0.1:8181", service)
        .await
        .unwrap();

    let response = coordinator.register(Uuid::new_v4(), "worker 1".into()).await;
    println!("Response from coordinator: {response:?}");
    sleep(Duration::from_millis(100000)).await;
}
