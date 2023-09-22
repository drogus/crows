use service::service;
use crate as utils;

// #[service(variant = "server", other_side = Worker)]
// pub trait Coordinator {
//     async fn hello(&self, name: String) -> String;
// }
//
// #[service(variant = "client", other_side = Coordinator)]
// pub trait Worker {
//     async fn hello(&self, name: String) -> String;
// }
//
// impl Coordinator for CoordinatorService {
//     async fn hello(&self, name: String) -> String {
//         format!("Hello from Coordinator {name}!")
//     }
// }
//
//
// impl Worker for WorkerService {
//     async fn hello(&self, name: String) -> String {
//         format!("Hello from Worker {name}!")
//     }
// }
