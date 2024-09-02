mod environment;
mod instance;
mod remote_io;
mod runtime;
mod wasi_host_ctx;

use anyhow::anyhow;
use crows_bindings::{HTTPError, HTTPMethod, HTTPRequest, HTTPResponse};
use crows_shared::Config;
use crows_utils::services::{IterationInfo, RequestInfo, RunId};
use crows_utils::{InfoHandle, InfoMessage};
use executors::Executors;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Body, Request, Url};
use serde_json::{from_slice, to_vec};
use std::collections::HashMap;
use std::str::FromStr;
use tokio::time::Instant;
use wasmtime::{Caller, Memory, Store};

pub mod executors;

pub use environment::Environment;
pub use instance::Instance;
pub use remote_io::RemoteIo;
pub use runtime::{Runtime, RuntimeInner, RuntimeMessage};
pub use wasi_host_ctx::WasiHostCtx;

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
    let func = instance
        .instance
        .get_typed_func::<(), ()>(&mut store, "scenario")?;

    if let Err(err) = func.call_async(&mut store, ()).await {
        if let Err(e) = store.data().stderr_sender.send(
            format!("Encountered an error when running a scenario: {err:?}")
                .as_bytes()
                .to_vec(),
        ) {
            eprintln!("Problem when sending logs to worker: {e:?}");
        }
    }

    Ok(())
}

pub async fn fetch_config(
    instance: Instance,
    mut store: &mut Store<WasiHostCtx>,
) -> anyhow::Result<crows_shared::Config> {
    let func = instance
        .instance
        .get_typed_func::<(), u32>(&mut store, "__config")?;

    let index = func.call_async(&mut store, ()).await?;
    let buffer = store
        .data_mut()
        .buffers
        .try_remove(index as usize)
        .ok_or(anyhow!("Couldn't find slab"))?;

    Ok(from_slice(&buffer)?)
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
