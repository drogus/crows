use crate::http_client::Client;
use crate::{Crows, CrowsPre, HostComponent};
use crate::{Environment, InfoHandle, RemoteIo, WasiHostCtx};
use anyhow::Result;
use crows_utils::InfoMessage;
use hyper_rustls::ConfigBuilderExt;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Arc;
use wasmtime::{Engine, Memory, MemoryType, Store};

// TODO: In the future I want the TLS settings to be configurable
lazy_static! {
    static ref TLS_CONFIG: Arc<rustls::ClientConfig> = Arc::new(
        rustls::ClientConfig::builder()
            .with_native_roots()
            .unwrap()
            .with_no_client_auth()
    );
}

pub struct Instance {
    pub instance: Crows,
}

impl Instance {
    pub fn new_store(
        engine: &Engine,
        env_vars: &HashMap<String, String>,
    ) -> Result<(Store<WasiHostCtx>, InfoHandle)> {
        let (stdout_sender, mut stdout_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (stderr_sender, mut stderr_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (request_info_sender, mut request_info_receiver) =
            tokio::sync::mpsc::unbounded_channel();

        let (info_sender, info_receiver) = tokio::sync::mpsc::unbounded_channel();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // If any of the send() methods returns an error we can break the loop as it
                    // means the listening part is dropped
                    Some(message) = stdout_receiver.recv() => if let Err(_) = info_sender.send(InfoMessage::Stdout(message)) { break },
                    Some(message) = stderr_receiver.recv() => if let Err(_) = info_sender.send(InfoMessage::Stderr(message)) { break },
                    Some(message) = request_info_receiver.recv() => if let Err(_) = info_sender.send(InfoMessage::RequestInfo(message)) { break },
                    else => break
                };
            }
        });

        let info_handle = InfoHandle {
            receiver: info_receiver,
        };

        let stdout = RemoteIo {
            sender: stdout_sender,
        };
        let stderr = RemoteIo {
            sender: stderr_sender.clone(),
        };

        let wasi = wasmtime_wasi::WasiCtxBuilder::new()
            .stdout(stdout)
            .stderr(stderr)
            .envs(
                &env_vars
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect::<Vec<(_, _)>>(),
            )
            .build();

        let tls_config = TLS_CONFIG.clone();

        let client = Client::new(tls_config);

        let host_ctx = WasiHostCtx {
            host: HostComponent::new(client, request_info_sender),
            wasi,
            table: wasmtime::component::ResourceTable::new(),
            memory: None,
            stderr_sender,
        };
        let mut store: Store<WasiHostCtx> = Store::new(engine, host_ctx);

        // TODO: we should limit memory to not allow too noisy neighbours
        let memory = Memory::new(&mut store, MemoryType::new(1, None)).unwrap();
        store.data_mut().memory = Some(memory);

        // WebAssembly execution will be paused for an async yield every time it
        // consumes 10000 fuel. Fuel will be refilled u64::MAX times.
        // TODO: test it in practice with CPU bound WASM
        store.fuel_async_yield_interval(Some(10000))?;
        store.set_fuel(u64::MAX).unwrap();

        Ok((store, info_handle))
    }

    pub async fn new(
        env: &Environment,
        component_pre: &CrowsPre<WasiHostCtx>,
        env_vars: &HashMap<String, String>,
    ) -> Result<(Self, InfoHandle, Store<WasiHostCtx>)> {
        let (mut store, info_handle) = Instance::new_store(&env.engine, env_vars)?;

        let instance = component_pre.instantiate_async(&mut store).await?;

        let result = Self { instance };
        Ok((result, info_handle, store))
    }

    pub async fn clear_connections(&mut self, store: &mut Store<WasiHostCtx>) {
        store.data_mut().host.clear_connections();
    }
}
