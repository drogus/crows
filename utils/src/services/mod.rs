use service::service;
use uuid::Uuid;
use crate as utils;
use thiserror::Error;
use serde::{Serialize, Deserialize};

#[derive(Error, Debug, Serialize, Deserialize, Clone)]
pub enum CoordinatorError {
    #[error("could not upload a module")]
    UploadModuleError,
}

#[derive(Error, Debug, Serialize, Deserialize, Clone)]
pub enum WorkerError {
    #[error("could not upload a module")]
    UploadModuleError,
}

// TODO: I don't like the fact that I need to specify the "other_side"
// here. It would be better if it was only needed when connecting, then
// all the trait definitions wouldn't have to be here
#[service(variant = "server", other_side = Worker)]
pub trait WorkerToCoordinator {
    async fn register(&self, uuid: Uuid, hostname: String) -> Result<(), CoordinatorError>;
}

#[service(variant = "server", other_side = Client)]
pub trait Coordinator {
    async fn upload_scenario(name: String, content: Vec<u8>) -> Result<(), CoordinatorError>;
}

#[service(variant = "client", other_side = WorkerToCoordinator)]
pub trait Worker {
    async fn upload_scenario(&mut self, name: String, content: Vec<u8>) -> Result<(), WorkerError>;
    async fn ping(&self) -> String;
    async fn start(&self, name: String);
}

#[service(variant = "client", other_side = Coordinator)]
pub trait Client {
    
}
