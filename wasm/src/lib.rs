use std::{any::Any, io::IoSlice, sync::Arc, collections::HashMap};
use tokio::time::{Duration, Instant};
use wasi_common::{
    file::{FdFlags, FileType},
    pipe::WritePipe,
    Table,
};
use wasmtime::{Config, Engine, Linker, Module, Store, Val};
use wasmtime_wasi::{tokio::WasiCtxBuilder, WasiCtx, WasiFile};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("the module with a given name couldn't be found")]
    NoSuchModule(String),
}

// A runtime should be run in a single async runtime. Ideally also in a single
// thread as we want a share-nothing architecture for performance and simplicity
struct Runtime {
    // we index instances with a name of the module, cause technically we can run
    // scenarios from multiple modules on a single runtime
    // TODO: might be simpler to assume only one module? I'm not sure yet if it's worth
    // it. If the overhead is too big I'd probably refactor it to allow only one module
    // at any point in time. I would like to start with multiple modules, though, to first
    // see if it's actually problematic, maybe it's not
    instances: HashMap<String, Vec<Instance>>,
}

impl Runtime {
    // TODO: it looks like the name/module pair should be in a separate data type, might
    //       be worth to extract it, although I'm not sure yet
    // TODO: if modules are indexed by name, how do we distinguish between module versions?
    //       maybe it should be a hash
    fn create_instances(
        &mut self,
        name: String,
        count: usize,
        instance: Instance,
    ) -> Result<(), Error> {
        let mut instances = self.instances.get_mut(&name).ok_or(Error::NoSuchModule(name))?;
        for _ in (0..count).into_iter() {
            instances.push(instance.clone());
        }

        Ok(())
    }
}

#[derive(Clone)]
struct Instance {
    engine: Engine,
    module: Module,
    linker: Linker<WasiCtx>,
}

impl Instance {
    pub fn new(raw_module: Vec<u8>) -> Result<Self, anyhow::Error> {
        let mut config = Config::new();
        config.async_support(true);
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;

        let module = Module::from_binary(&engine, &raw_module)?;

        let mut linker = Linker::new(&engine);
        let get_row_count_type = wasmtime::FuncType::new(None, Some(wasmtime::ValType::I32));

        let _ = linker.func_new_async(
            "crows",
            "hello",
            get_row_count_type,
            |_, _params, results| {
                Box::new(async {
                    println!("Waiting 5s in rust async code");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    println!("Finished waiting");
                    results[0] = Val::I32(1111 as i32);
                    Ok(())
                })
            },
        )?;
        linker.func_wrap("crows", "print", |param: i32| {
            println!("Got value: {param}")
        })?;

        wasmtime_wasi::tokio::add_to_linker(&mut linker, |cx| cx)?;

        Ok(Self {
            engine,
            module,
            linker,
        })
    }
}

#[derive(Clone)]
struct RemoteStdout {
    sender: std::sync::mpsc::Sender<Vec<u8>>,
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
            self.sender.send(slice);
        }
        Ok(size)
    }
}
