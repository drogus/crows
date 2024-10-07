mod environment;
mod http_client;
mod instance;
mod remote_io;
mod runtime;
mod wasi_host_ctx;
pub mod http;

use std::time::Duration;

use crows_shared::ConstantArrivalRateConfig;
use crows_utils::services::{RequestInfo, RunId};
use crows_utils::{InfoHandle, InfoMessage};
use executors::Executors;
use http_client::Client;
use tokio::sync::mpsc::UnboundedSender;

pub mod executors;

pub use environment::Environment;
pub use instance::Instance;
pub use remote_io::RemoteIo;
pub use runtime::{Runtime, RuntimeInner, RuntimeMessage};
pub use wasi_host_ctx::WasiHostCtx;

use wasi::http::types::{self as http, HostOutgoingRequest};

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
            format!("Encountered an error when running a scenario: {err:?}")
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
    todo!()
    // let config = instance.instance.call_get_config(&mut store).await?;
    // let config = match config {
    //     runtime::local::crows::types::Config::ConstantArrivalRate(c) => {
    //         crows_shared::Config::ConstantArrivalRate(ConstantArrivalRateConfig {
    //             duration: Duration::from_millis(c.duration),
    //             rate: c.rate as usize,
    //             time_unit: Duration::from_millis(c.time_unit),
    //             allocated_vus: c.allocated_vus as usize,
    //             graceful_shutdown_timeout: Duration::from_millis(c.graceful_shutdown_timeout),
    //         })
    //     }
    // };
    // Ok(config)
}

pub async fn run_scenario(runtime: Runtime, config: crows_shared::Config) {
    todo!()
    // let info_sender = runtime.info_sender.clone();
    // let mut executor = Executors::create_executor(config, runtime).await;
    //
    // tokio::spawn(async move {
    //     // TODO: prepare should be an entirely separate step and coordinator should wait for
    //     // prepare from all of the workers
    //     if let Err(err) = executor.prepare().await {
    //         let message = format!("Executor's prepare() function errored out: {err:?}");
    //         eprintln!("{message}");
    //         if let Err(err) = info_sender.send(InfoMessage::PrepareError(message)) {
    //             eprintln!("Couldn't send InfoMessage::PrepareError message to the coordinator. Error: {err:?}");
    //         }
    //     }
    //     if let Err(err) = executor.run().await {
    //         let message = format!("Executor's run() function errored out: {err:?}");
    //         eprintln!("{message}");
    //         if let Err(err) = info_sender.send(InfoMessage::RunError(message)) {
    //             eprintln!("Couldn't send InfoMessage::RunError message to the coordinator. Error: {err:?}");
    //         }
    //     }
    // });
}
