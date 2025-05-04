mod environment;
mod http_client;
mod instance;
mod remote_io;
mod runtime;
mod wasi_host_ctx;

use std::collections::HashMap;
use std::time::Duration;

use crows_shared::{Config, ConstantArrivalRateConfig};
use crows_utils::services::RunId;
use crows_utils::{InfoHandle, InfoMessage};
use executors::Executors;
use serde::{Deserialize, Serialize};
use wasmtime::{Caller, Memory, Store};

pub mod executors;

pub use environment::Environment;
pub use instance::Instance;
pub use remote_io::RemoteIo;
pub use runtime::{Runtime, RuntimeInner, RuntimeMessage};
pub use wasi_host_ctx::WasiHostCtx;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum HTTPMethod {
    HEAD,
    GET,
    POST,
    PATCH,
    PUT,
    DELETE,
    OPTIONS,
    TRACE,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct HTTPRequest {
    // TODO: these should not be public I think, I'd prefer to do a public interface for them
    pub url: String,
    pub method: HTTPMethod,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HTTPError {
    pub message: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct HTTPResponse {
    // TODO: these should not be public I think, I'd prefer to do a public interface for them
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub status: u16,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("the module with a given name couldn't be found")]
    NoSuchRun(RunId),
}

pub fn get_memory<T>(caller: &mut Caller<'_, T>) -> anyhow::Result<Memory> {
    Ok(caller.get_export("memory").unwrap().into_memory().unwrap())
}

pub async fn run_wasm(
    instance: &mut Instance,
    mut store: &mut Store<WasiHostCtx>,
) -> anyhow::Result<()> {
    if let Err(err) = instance.instance.call_run_scenario(&mut store).await {
        if let Err(e) = store.data().stderr_sender.send(
            // TODO: check the type of original error - we should probably display the
            // error if it's not an error from the scenario
            format!("Encountered an error when running a scenario.")
                .as_bytes()
                .to_vec(),
        ) {
            eprintln!("Problem when sending logs to worker: {e:?}");
        }
    }

    instance.clear_connections(&mut store).await;

    Ok(())
}

pub async fn fetch_config(
    instance: Instance,
    mut store: &mut Store<WasiHostCtx>,
) -> anyhow::Result<crows_shared::Config> {
    let config = instance.instance.call_get_config(&mut store).await?;
    let config = match config {
        runtime::local::crows::types::Config::ConstantArrivalRate(c) => {
            Config::ConstantArrivalRate(ConstantArrivalRateConfig {
                duration: Duration::from_millis(c.duration),
                rate: c.rate as usize,
                time_unit: Duration::from_millis(c.time_unit),
                allocated_vus: c.allocated_vus as usize,
                graceful_shutdown_timeout: Duration::from_millis(c.graceful_shutdown_timeout)
            })
        }
    };
    Ok(config)
}

pub async fn run_scenario(runtime: Runtime, config: Config) {
    let info_sender = runtime.info_sender.clone();
    let mut executor = Executors::create_executor(config, runtime).await;

    tokio::spawn(async move {
        // TODO: prepare should be an entirely separate step and coordinator should wait for
        // prepare from all of the workers
        if let Err(err) = executor.prepare().await {
            let message = format!("Executor's prepare() function errored out: {err:?}");
            eprintln!("{message}");
            if let Err(err) = info_sender.send(InfoMessage::PrepareError(message)) {
                eprintln!("Couldn't send InfoMessage::PrepareError message to the coordinator. Error: {err:?}");
            }
        }
        if let Err(err) = executor.run().await {
            let message = format!("Executor's run() function errored out: {err:?}");
            eprintln!("{message}");
            if let Err(err) = info_sender.send(InfoMessage::RunError(message)) {
                eprintln!("Couldn't send InfoMessage::RunError message to the coordinator. Error: {err:?}");
            }
        }
    });
}
