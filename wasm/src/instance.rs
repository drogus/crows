use anyhow::Result;
use std::collections::HashMap;
use wasmtime::{Engine, Instance as WasmtimeInstance, Memory, MemoryType, Module, Store};
use crate::{Environment, InfoHandle, RemoteIo, WasiHostCtx};
use crows_utils::InfoMessage;

pub struct Instance {
    pub instance: WasmtimeInstance,
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

        let wasi_ctx = wasmtime_wasi::WasiCtxBuilder::new()
            .stdout(stdout)
            .stderr(stderr)
            .envs(&env_vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect::<Vec<(_, _)>>())
            .build();

        let host_ctx = WasiHostCtx {
            preview2_ctx: wasi_ctx,
            preview2_table: wasmtime::component::ResourceTable::new(),
            preview1_adapter: wasmtime_wasi::preview1::WasiPreview1Adapter::new(),
            buffers: slab::Slab::default(),
            memory: None,
            client: reqwest::Client::new(),
            request_info_sender,
            stderr_sender,
        };
        let mut store: Store<WasiHostCtx> = Store::new(engine, host_ctx);

        let memory = Memory::new(&mut store, MemoryType::new(1, None)).unwrap();
        store.data_mut().memory = Some(memory);

        // WebAssembly execution will be paused for an async yield every time it
        // consumes 10000 fuel. Fuel will be refilled u64::MAX times.
        store.fuel_async_yield_interval(Some(10000))?;
        store.set_fuel(u64::MAX).unwrap();

        Ok((store, info_handle))
    }

    pub async fn new(
        env: &Environment,
        module: &Module,
        env_vars: &HashMap<String, String>,
    ) -> Result<(Self, InfoHandle, Store<WasiHostCtx>)> {
        let (mut store, info_handle) = Instance::new_store(&env.engine, env_vars)?;
        let instance = env.linker.instantiate_async(&mut store, module).await?;

        let result = Self { instance };
        Ok((result, info_handle, store))
    }
}
