#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

use std::time::Duration;

use tokio::time::sleep;
use utils::services::{create_worker_to_coordinator_server, CoordinatorError, create_coordinator_server};
use utils::services::{Worker, WorkerToCoordinator, Coordinator};
use uuid::Uuid;

#[derive(Clone)]
struct WorkerData {
    hostname: String,
    uuid: Uuid,
}

#[derive(Clone)]
struct WorkerToCoordinatorService {
    workers: Vec<WorkerData>
}

impl WorkerToCoordinator for WorkerToCoordinatorService {
    async fn register(&self, uuid: Uuid, hostname: String) -> Result<(), CoordinatorError> {
        println!("register worker {uuid}, hostname: {hostname}");

        Ok(())
    }
}

#[derive(Clone)]
struct CoordinatorService {
}

impl Coordinator for CoordinatorService {
    async fn upload_scenario(&self, name:String, content:Vec<u8>) -> Result<(), CoordinatorError> {
        Ok(())
    }
}

#[tokio::main]
pub async fn main() {
    tokio::spawn(async {
        let server = create_worker_to_coordinator_server("127.0.0.1:8181")
            .await
            .unwrap();
        let service = WorkerToCoordinatorService {
            workers: Vec::new()
        };
        while let Some(mut client) = server.accept(service.clone()).await {
            println!("Worker connected");
            // let response = worker_client.hello("coordinator".into()).await;
            // println!("Response from worker: {response:?}");
            //
            client.upload_scenario("foo".into(), Vec::new()).await;
            println!("Done");
            client.wait().await;
            println!("Worker disconnected");
        }
    });

    tokio::spawn(async {
        let server = create_coordinator_server("127.0.0.1:8282")
            .await
            .unwrap();
        let service = CoordinatorService {};
        while let Some(mut client) = server.accept(service.clone()).await {
            tokio::spawn(async move {
                // we don't send any messages for the client, so in order to not drop it
                // (and thus disconnect), we need to wait
                client.wait().await;
            });
        }
    });

    sleep(Duration::from_millis(100000)).await;
}
