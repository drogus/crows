use anyhow::anyhow;
use crows_bindings::{HTTPError, HTTPMethod, HTTPRequest, HTTPResponse};
use crows_utils::services::RunId;
use futures::Future;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Body, Request, Url};
use serde::Serialize;
use serde_json::{from_slice, to_vec};
use std::collections::VecDeque;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::{any::Any, collections::HashMap, io::IoSlice};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Mutex;
use wasi_common::file::{FdFlags, FileType};
use wasi_common::WasiFile;
use wasmtime::{Caller, Engine, Linker, Memory, MemoryType, Module, Store};
use wasmtime_wasi::{StdoutStream, StreamResult};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("the module with a given name couldn't be found")]
    NoSuchRun(RunId),
}

pub struct Runtime<'a> {
    // we index instances with the run id, cause technically we can run
    // scenarios from multiple modules on a single runtime
    // TODO: might be simpler to assume only one module? I'm not sure yet if it's worth
    // it. If the overhead is too big I'd probably refactor it to allow only one module
    // at any point in time. I would like to start with multiple modules, though, to first
    // see if it's actually problematic, maybe it's not and it seems to give more flexibility
    pub instances: VecDeque<Instance<'a>>,
    pub environment: Environment,
}

impl<'a> Runtime<'a> {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            instances: VecDeque::new(),
            environment: Environment::new()?,
        })
    }

    pub async fn compile_instance(&self, content: &Vec<u8>) -> anyhow::Result<(Instance<'_>, UnboundedReceiver<Vec<u8>>)> {
        Instance::new(content, &self.environment).await
    }
    // TODO: it looks like the id/module pair should be in a separate data type, might
    //       be worth to extract it in the future
    // pub fn create_instances(
    //     &mut self,
    //     id: RunId,
    //     count: usize,
    //     instance: &'a Instance<'_>,
    // ) -> Result<(), Error> {
    //     let instances = self.instances.get_mut(&id).ok_or(Error::NoSuchRun(id))?;
    //     for _ in (0..count).into_iter() {
    //         instances.push(instance);
    //     }
    //
    //     Ok(())
    // }
}

struct WasiHostCtx {
    preview2_ctx: wasmtime_wasi::WasiCtx,
    preview2_table: wasmtime::component::ResourceTable,
    preview1_adapter: wasmtime_wasi::preview1::WasiPreview1Adapter,
    memory: Option<Memory>,
    buffers: slab::Slab<Box<[u8]>>,
    client: reqwest::Client,
}

fn create_return_value(status: u8, length: u32, ptr: u32) -> u64 {
    assert!(
        length <= 0x00FFFFFF,
        "Length must be no larger than 3 bytes"
    );
    ((status as u64) << 56) | ((length as u64) << 32) | (ptr as u64)
}

impl WasiHostCtx {
    pub fn instantiate(&mut self, mem: Memory) {
        self.memory = Some(mem);
    }

    pub async fn wrap_async<'a, U, F, E>(
        mut caller: Caller<'a, Self>,
        ptr: u32,
        len: u32,
        f: F,
    ) -> anyhow::Result<u64>
    where
        F: for<'b> FnOnce(
            &'b mut Caller<'_, Self>,
            HTTPRequest,
        ) -> Pin<Box<dyn Future<Output = Result<U, E>> + 'b + Send>>,
        U: Serialize,
        E: Serialize,
    {
        let memory = get_memory(&mut caller)?;

        let slice = memory
            .data(&caller)
            .get(ptr as usize..(ptr + len) as usize)
            .ok_or(anyhow!("Could not get memory slice"))?;

        let arg = from_slice(slice)?;

        let result = f(&mut caller, arg).await;

        let (_, store) = { memory.data_and_store_mut(&mut caller) };

        match result {
            Ok(ret) => {
                let encoded = to_vec(&ret).unwrap();

                let length = encoded.len();
                let index = store.buffers.insert(encoded.into_boxed_slice());

                Ok(create_return_value(0, length as u32, index as u32))
            }
            Err(err) => {
                let encoded = to_vec(&err).unwrap();

                let length = encoded.len();
                let index = store.buffers.insert(encoded.into_boxed_slice());

                Ok(create_return_value(0, length as u32, index as u32))
            }
        }
    }

    pub fn http<'a>(
        mut caller: &'a mut Caller<'_, Self>,
        request: HTTPRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HTTPResponse, HTTPError>> + 'a + Send>> {
        Box::pin(async move {
            let memory = get_memory(&mut caller).unwrap();
            let (_, store) = memory.data_and_store_mut(&mut caller);

            let client = &store.client;

            let method = match request.method {
                HTTPMethod::HEAD => reqwest::Method::HEAD,
                HTTPMethod::GET => reqwest::Method::GET,
                HTTPMethod::POST => reqwest::Method::POST,
                HTTPMethod::PUT => reqwest::Method::PUT,
                HTTPMethod::DELETE => reqwest::Method::DELETE,
                HTTPMethod::OPTIONS => reqwest::Method::OPTIONS,
            };
            let url = Url::parse(&request.url).map_err(|err| HTTPError {
                message: format!("Error when parsing the URL: {err:?}"),
            })?;

            let mut reqw_req = Request::new(method, url);

            for (key, value) in request.headers {
                let name = HeaderName::from_str(&key).map_err(|err| HTTPError {
                    message: format!("Invalid header name: {key}: {err:?}"),
                })?;
                let value = HeaderValue::from_str(&value).map_err(|err| HTTPError {
                    message: format!("Invalid header value: {value}: {err:?}"),
                })?;
                reqw_req.headers_mut().insert(name, value);
            }

            *reqw_req.body_mut() = request.body.map(|b| Body::from(b));

            let response = client.execute(reqw_req).await.map_err(|err| HTTPError {
                message: format!("Error when sending a request: {err:?}"),
            })?;

            let mut headers = HashMap::new();
            for (name, value) in response.headers().iter() {
                let value = value.to_str().map_err(|err| HTTPError {
                    message: format!("Could not parse response header {value:?}: {err:?}"),
                })?;
                headers.insert(name.to_string(), value.to_string());
            }

            let status = response.status().as_u16();
            let body = response.text().await.map_err(|err| HTTPError {
                message: format!("Problem with fetching the body: {err:?}"),
            })?;

            Ok(HTTPResponse {
                headers,
                body,
                status,
            })
        })
    }

    pub fn set_config(mut caller: Caller<'_, Self>, ptr: u32, len: u32) -> anyhow::Result<u32> {
        let memory = get_memory(&mut caller)?;

        let slice = memory
            .data(&caller)
            .get(ptr as usize..(ptr + len) as usize)
            .ok_or(anyhow!("Could not get memory slice"))?
            .to_owned()
            .into_boxed_slice();

        let (_, store) = memory.data_and_store_mut(&mut caller);

        let index = store.buffers.insert(slice);

        Ok(index as u32)
    }

    pub fn consume_buffer(
        mut caller: Caller<'_, Self>,
        index: u32,
        ptr: u32,
        len: u32,
    ) -> anyhow::Result<()> {
        let memory = get_memory(&mut caller)?;
        let (slice, store) = memory.data_and_store_mut(&mut caller);

        let buffer = store.buffers.try_remove(index as usize).unwrap();
        anyhow::ensure!(
            len as usize == buffer.len(),
            "bad length passed to consume_buffer"
        );
        slice
            .get_mut((ptr as usize)..((ptr + len) as usize))
            .unwrap()
            .copy_from_slice(&buffer);

        Ok(())
    }
}

impl wasmtime_wasi::WasiView for WasiHostCtx {
    fn table(&mut self) -> &mut wasmtime::component::ResourceTable {
        &mut self.preview2_table
    }

    fn ctx(&mut self) -> &mut wasmtime_wasi::WasiCtx {
        &mut self.preview2_ctx
    }
}

impl wasmtime_wasi::preview1::WasiPreview1View for WasiHostCtx {
    fn adapter(&self) -> &wasmtime_wasi::preview1::WasiPreview1Adapter {
        &self.preview1_adapter
    }

    fn adapter_mut(&mut self) -> &mut wasmtime_wasi::preview1::WasiPreview1Adapter {
        &mut self.preview1_adapter
    }
}

#[derive(Clone)]
pub struct Environment {
    engine: Engine,
    linker: Linker<WasiHostCtx>,
}

pub struct Instance<'a> {
    engine: &'a Engine,
    module: Module,
    linker: &'a Linker<WasiHostCtx>,
    store: wasmtime::Store<WasiHostCtx>,
    instance: wasmtime::Instance,
}

impl Environment {
    pub fn new() -> anyhow::Result<Self> {
        let mut config = wasmtime::Config::new();
        config.async_support(true);
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;

        let mut linker = Linker::new(&engine);

        linker
            .func_wrap("crows", "consume_buffer", WasiHostCtx::consume_buffer)
            .unwrap();
        linker
            .func_wrap2_async("crows", "http", |caller, ptr, len| {
                Box::new(async move {
                    WasiHostCtx::wrap_async(caller, ptr, len, WasiHostCtx::http).await
                })
            })
            .unwrap();
        linker
            .func_wrap("crows", "set_config", WasiHostCtx::set_config)
            .unwrap();

        wasmtime_wasi::preview1::add_to_linker_async(&mut linker)?;

        Ok(Self { engine, linker })
    }
}

pub fn get_memory<T>(caller: &mut Caller<'_, T>) -> anyhow::Result<Memory> {
    Ok(caller.get_export("memory").unwrap().into_memory().unwrap())
}

impl<'a> Instance<'a> {
    pub fn new_store(
        engine: &Engine,
    ) -> anyhow::Result<(wasmtime::Store<WasiHostCtx>, UnboundedReceiver<Vec<u8>>)> {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();

        let stdout = RemoteStdout { sender };

        let wasi_ctx = wasmtime_wasi::WasiCtxBuilder::new()
            .stdout(stdout)
            // .inherit_stdio()
            .build();

        let host_ctx = WasiHostCtx {
            preview2_ctx: wasi_ctx,
            preview2_table: wasmtime::component::ResourceTable::new(),
            preview1_adapter: wasmtime_wasi::preview1::WasiPreview1Adapter::new(),
            buffers: slab::Slab::default(),
            memory: None,
            client: reqwest::Client::new(),
        };
        let mut store: Store<WasiHostCtx> = Store::new(engine, host_ctx);

        let memory = Memory::new(&mut store, MemoryType::new(1, None)).unwrap();
        store.data_mut().memory = Some(memory);

        // WebAssembly execution will be paused for an async yield every time it
        // consumes 10000 fuel. Fuel will be refilled u64::MAX times.
        store.fuel_async_yield_interval(Some(10000))?;
        store.set_fuel(u64::MAX).unwrap();

        Ok((store, receiver))
    }

    pub async fn new(raw_module: &Vec<u8>, env: &'a Environment) -> anyhow::Result<(Self, UnboundedReceiver<Vec<u8>>)> {
        let module = Module::from_binary(&env.engine, raw_module)?;

        let (mut store, logs_receiver) = Instance::new_store(&env.engine)?;
        let instance = env.linker.instantiate_async(&mut store, &module).await?;

        // let func = instance
        // .clone()
        // .get_typed_func::<(), ()>(&mut store, "test")?;

        // func.call_async(&mut store, ()).await?;

        // drop(store);

        let result = Self {
            engine: &env.engine,
            module,
            linker: &env.linker,
            store,
            instance,
        };
        Ok((result, logs_receiver))
    }
}

pub async fn run_wasm(instance: &mut Instance<'_>) -> anyhow::Result<()> {
    let func = instance
        .instance
        .get_typed_func::<(), ()>(&mut instance.store, "test")?;

    func.call_async(&mut instance.store, ()).await?;

    Ok(())
}

pub async fn fetch_config(instance: &mut Instance<'_>) -> anyhow::Result<crows_shared::Config> {
    let func = instance
        .instance
        .get_typed_func::<(), u32>(&mut instance.store, "__config")?;

    let index = func.call_async(&mut instance.store, ()).await?;
    let buffer = instance
        .store
        .data_mut()
        .buffers
        .try_remove(index as usize)
        .ok_or(anyhow!("Couldn't find slab"))?;

    Ok(from_slice(&buffer)?)
}

#[derive(Clone)]
struct RemoteStdout {
    sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
}

#[wiggle::async_trait]
impl WasiFile for RemoteStdout {
    fn as_any(&self) -> &dyn Any {
        self
    }
    async fn get_filetype(&self) -> Result<FileType, wasi_common::Error> {
        Ok(FileType::Pipe)
    }
    async fn get_fdflags(&self) -> Result<FdFlags, wasi_common::Error> {
        Ok(FdFlags::APPEND)
    }
    async fn write_vectored<'a>(&self, bufs: &[IoSlice<'a>]) -> Result<u64, wasi_common::Error> {
        let mut size: u64 = 0;
        for slice in bufs {
            let slice = slice.to_vec();
            size += slice.len() as u64;
            self.sender.send(slice).unwrap();
        }
        Ok(size)
    }
}

impl wasmtime_wasi::HostOutputStream for RemoteStdout {
    fn write(&mut self, bytes: bytes::Bytes) -> StreamResult<()> {
        self.sender.send(bytes.to_vec()).unwrap();

        Ok(())
    }

    fn flush(&mut self) -> StreamResult<()> {
        Ok(())
    }

    fn check_write(&mut self) -> StreamResult<usize> {
        Ok(1024 * 1024)
    }
}

impl StdoutStream for RemoteStdout {
    fn stream(&self) -> Box<dyn wasmtime_wasi::HostOutputStream> {
        Box::new(self.clone())
    }

    fn isatty(&self) -> bool {
        false
    }
}

#[async_trait::async_trait]
impl wasmtime_wasi::Subscribe for RemoteStdout {
    async fn ready(&mut self) {}
}
