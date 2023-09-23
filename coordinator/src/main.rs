#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

use std::time::Duration;

use tokio::time::sleep;
use utils::services::{create_coordinator_server, connect_to_coordinator};
use utils::services::{Worker, Coordinator};

#[derive(Clone)]
struct CoordinatorService;

impl Coordinator for CoordinatorService {
    async fn hello(&self, name: String) -> String {
        format!("Hello from Coordinator {name}!")
    }
}

struct WorkerService;

impl Worker for WorkerService {
    async fn hello(&self, name: String) -> String {
        format!("Hello from Worker {name}!")
    }
}

#[tokio::main]
pub async fn main() {
    tokio::spawn(async {
        let server = create_coordinator_server("127.0.0.1:8181").await.unwrap();
        let service = CoordinatorService {};
        while let Some(worker_client) = server.accept(service.clone()).await {
            let response = worker_client.hello("coordinator".into()).await;
            println!("Response from worker: {response:?}");
        }
    });

    sleep(Duration::from_millis(100)).await;
    let service = WorkerService {};
    let world_client = connect_to_coordinator("127.0.0.1:8181", service).await.unwrap();

    let response = world_client.hello("worker 1".into()).await;
    println!("Response from coordinator: {response:?}");

    sleep(Duration::from_millis(100000)).await;
}


