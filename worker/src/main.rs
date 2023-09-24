#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

use std::time::Duration;

use tokio::time::sleep;
use utils::services::connect_to_worker_to_coordinator;
use utils::services::{Worker, WorkerToCoordinator};

struct WorkerService;

impl Worker for WorkerService {
    async fn hello(&self, name: String) -> String {
        format!("Hello from Worker {name}!")
    }
}

#[tokio::main]
pub async fn main() {
    let service = WorkerService {};
    let world_client = connect_to_worker_to_coordinator("127.0.0.1:8181", service)
        .await
        .unwrap();

    let response = world_client.hello("worker 1".into()).await;
    println!("Response from coordinator: {response:?}");
    sleep(Duration::from_millis(100000)).await;
}
