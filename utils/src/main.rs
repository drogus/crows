// #![feature(return_position_impl_trait_in_trait)]
// #![feature(async_fn_in_trait)]
//
// use futures::prelude::*;
// use tokio::time::{sleep, Duration};
//
// use service::service;
//
// // TODO: implement default associated types for Coordinator, so service can use them for impl
// // implement a proc macro to implement Service
//
// #[service(variant = "server", other_side = Worker)]
// trait Coordinator {
//     async fn hello(&self, name: String) -> String;
// }
//
// #[derive(Clone)]
// struct CoordinatorService;
//
// impl Coordinator for CoordinatorService {
//     async fn hello(&self, name: String) -> String {
//         format!("Hello from Coordinator {name}!")
//     }
// }
//
// #[service(variant = "client", other_side = Coordinator)]
// pub trait Worker {
//     async fn hello(&self, name: String) -> String;
// }
//
// #[derive(Clone)]
// struct WorkerService;
//
// impl Worker for WorkerService {
//     async fn hello(&self, name: String) -> String {
//         format!("Hello from Worker {name}!")
//     }
// }
//
// #[tokio::main]
// pub async fn main() {
//     tokio::spawn(async {
//         let server = create_coordinator_server("127.0.0.1:8181").await.unwrap();
//         let service = CoordinatorService {};
//         while let Some(worker_client) = server.accept(service.clone()).await {
//             let response = worker_client.hello("coordinator".into()).await;
//             println!("Response from worker: {response:?}");
//         }
//     });
//
//     sleep(Duration::from_millis(100)).await;
//     let service = WorkerService {};
//     let world_client = connect_to_coordinator("127.0.0.1:8181", service)
//         .await
//         .unwrap();
//
//     let response = world_client.hello("worker 1".into()).await;
//     println!("Response from coordinator: {response:?}");
//
//     sleep(Duration::from_millis(100000)).await;
// }
