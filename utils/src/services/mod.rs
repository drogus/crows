use crate::{self as utils};
use crows_service::service;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug, Serialize, Deserialize, Clone)]
pub enum CoordinatorError {
    #[error("could not upload a module")]
    UploadModuleError,
    #[error("couldn't find module {0}")]
    NoSuchModule(String),
    #[error("Failed to create runtime: {0}")]
    FailedToCreateRuntime(String),
    #[error("Failed to compile module")]
    FailedToCompileModule,
    #[error("Couldn't fetch config: {0}")]
    CouldNotFetchConfig(String),
}

#[derive(Error, Debug, Serialize, Deserialize, Clone)]
pub enum WorkerError {
    #[error("could not upload a module")]
    UploadModuleError,
    #[error("could not find a requested scenario")]
    ScenarioNotFound,
    #[error("could not create a module from binary")]
    CouldNotCreateModule,
    #[error("could not create runtime: {0}")]
    CouldNotCreateRuntime(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RunId(Uuid);

impl RunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Into<Uuid> for RunId {
    fn into(self) -> Uuid {
        self.0
    }
}

// TODO: I don't like the fact that I need to specify the "other_side"
// here. It would be better if it was only needed when connecting, then
// all the trait definitions wouldn't have to be here
#[service(variant = "server", other_side = Worker)]
pub trait WorkerToCoordinator {
    async fn ping(&self) -> String;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WorkerStatus {
    Available,
    Busy,
}

#[service(variant = "server", other_side = Client)]
pub trait Coordinator {
    async fn upload_scenario(name: String, content: Vec<u8>) -> Result<(), CoordinatorError>;
    async fn start(name: String, workers_number: usize) -> Result<(), CoordinatorError>;
    async fn list_workers() -> Vec<String>;
    async fn update_status(&self, status: WorkerStatus, id: Uuid);
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkerData {
    pub id: Uuid,
    pub hostname: String,
}

#[service(variant = "client", other_side = WorkerToCoordinator)]
pub trait Worker {
    async fn upload_scenario(&self, name: String, content: Vec<u8>);
    async fn ping(&self) -> String;
    async fn start(&self, name: String, config: crows_shared::Config) -> Result<(), WorkerError>;
    async fn get_data(&self) -> WorkerData;
}

#[service(variant = "client", other_side = Coordinator)]
pub trait Client {}
