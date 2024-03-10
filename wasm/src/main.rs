// use std::{any::Any, io::IoSlice, sync::Arc};
// use tokio::time::{Duration, Instant};
// use wasi_common::{
//     file::{FdFlags, FileType},
//     pipe::WritePipe,
//     Table,
// };
// use wasmtime::{Config, Engine, Linker, Module, Store, Val};
// use wasmtime_wasi::{tokio::WasiCtxBuilder, WasiCtx, WasiFile};

use std::time::Instant;

use crows_wasm::{run_wasm, Instance};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    let path = std::env::var("MODULE_PATH").expect("MODULE_PATH env var is not set");
    let content = std::fs::read(path).unwrap();

    // let mut config = Config::new();
    // config.async_support(true);
    // config.consume_fuel(true);

    // let engine = Engine::new(&config)?;

    // let module = Module::from_binary(&engine, &content)?;

    let env = Instance::new(content.clone())?;
    let instant = Instant::now();
    for _ in 0..1 {

        run_wasm(&env).await?;
    }

    println!("elapsed: {}ms", instant.elapsed().as_millis());

    Ok(())
}

// struct Environment {
//     engine: Engine,
//     module: Module,
//     linker: Arc<Linker<WasiCtx>>,
// }
//
// impl Environment {
//     pub fn new(module: Module, engine: Engine) -> Result<Self, anyhow::Error> {
//         let mut linker = Linker::new(&engine);
//         let get_row_count_type = wasmtime::FuncType::new(None, Some(wasmtime::ValType::I32));
//
//         let _ = linker.func_new_async(
//             "crows",
//             "hello",
//             get_row_count_type,
//             |_, _params, results| {
//                 Box::new(async {
//                     println!("Waiting 5s in rust async code");
//                     tokio::time::sleep(Duration::from_secs(5)).await;
//                     println!("Finished waiting");
//                     results[0] = Val::I32(1111 as i32);
//                     Ok(())
//                 })
//             },
//         )?;
//         linker.func_wrap("crows", "print", |param: i32| {
//             println!("Got value: {param}")
//         })?;
//
//         wasmtime_wasi::tokio::add_to_linker(&mut linker, |cx| cx)?;
//
//         Ok(Self {
//             engine,
//             module,
//             linker: Arc::new(linker),
//         })
//     }
// }
//
// #[derive(Clone)]
// struct RemoteStdout {
//     sender: std::sync::mpsc::Sender<Vec<u8>>,
// }
//
// #[wiggle::async_trait]
// impl WasiFile for RemoteStdout {
//     fn as_any(&self) -> &dyn Any {
//         self
//     }
//     async fn get_filetype(&self) -> Result<FileType, wasi_common::Error> {
//         Ok(FileType::Pipe)
//     }
//     async fn get_fdflags(&self) -> Result<FdFlags, wasi_common::Error> {
//         Ok(FdFlags::APPEND)
//     }
//     async fn write_vectored<'a>(&self, bufs: &[IoSlice<'a>]) -> Result<u64, wasi_common::Error> {
//         let mut size: u64 = 0;
//         for slice in bufs {
//             let slice = slice.to_vec();
//             size += slice.len() as u64;
//             self.sender.send(slice);
//         }
//         Ok(size)
//     }
// }
//
// async fn run_wasm(env: &Environment) -> Result<(), anyhow::Error> {
//     let (sender, receiver) = std::sync::mpsc::channel();
//     tokio::spawn(async move {
//         tokio::task::spawn_blocking(move || {
//             while let Ok(message) = receiver.recv() {
//                 println!("stdout: {}", String::from_utf8(message).unwrap());
//             }
//         })
//         .await;
//     });
//
//     let stdout = RemoteStdout { sender };
//     let wasi = WasiCtxBuilder::new()
//         .stdout(Box::new(stdout.clone()))
//         // Set an environment variable so the wasm knows its name.
//         // .env("NAME", &inputs.name)?
//         .build();
//     let mut store = Store::new(&env.engine, wasi);
//
//     // WebAssembly execution will be paused for an async yield every time it
//     // consumes 10000 fuel. Fuel will be refilled u64::MAX times.
//     store.fuel_async_yield_interval(Some(10000))?;
//
//     // Instantiate into our own unique store using the shared linker, afterwards
//     // acquiring the `_start` function for the module and executing it.
//     let instance = env
//         .linker
//         .instantiate_async(&mut store, &env.module)
//         .await?;
//
//     let func = instance
//         .clone()
//         .get_typed_func::<(), ()>(&mut store, "test")?;
//
//     func.call_async(&mut store, ()).await?;
//
//     drop(store);
//
//     Ok(())
// }



