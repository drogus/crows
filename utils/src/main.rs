use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use lunatic::{Process, Tag, Mailbox};
use lunatic_process::env::Environment;
use lunatic_process::message::{Message, DataMessage};
use lunatic_process::state::ProcessState;
use lunatic_process::{
    env::{Environments, LunaticEnvironments},
    runtimes::{self},
    wasm::spawn_wasm,
};
use lunatic_process_api::ProcessConfigCtx;
use lunatic_runtime::{DefaultProcessConfig, DefaultProcessState};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, UnboundedReceiver};

#[derive(Serialize, Deserialize, Debug)]
struct Foo {
    name: String,
    process: Process<Foo>,
}

#[derive(Deserialize)]
struct WasmModuleMessage {
    name: String,
    module: Vec<u8>,
    concurrency: usize,
}

async fn run_wasm_module(path: &str, mut receiver: UnboundedReceiver<WasmModuleMessage>) -> Result<()> {
    let wasmtime_config = runtimes::wasmtime::default_config();
    let runtime = runtimes::wasmtime::WasmtimeRuntime::new(&wasmtime_config)?;

    let envs = Arc::new(LunaticEnvironments::default());
    let env = envs.create(1).await;

    let mut config = DefaultProcessConfig::default();
    config.set_can_spawn_processes(true);
    config.set_can_compile_modules(true);

    let module = std::fs::read(path).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => anyhow!("Module '{}' not found", path),
        _ => anyhow!("Failed to read module: {}", err),
    })?;

    let module = Arc::new(runtime.compile_module::<DefaultProcessState>(module.into())?);

    let state = DefaultProcessState::new(
        env.clone(),
        None,
        runtime.clone(),
        module.clone(),
        Arc::new(config),
        Default::default(),
    )
    .map_err(|err| anyhow!("Failed to create process state: {}", err))?;

    env.can_spawn_next_process()
        .await
        .context("Failed to check if spawning is allowed")?;

    let mailbox_clone = state.message_mailbox().clone();
    let (task, process) = spawn_wasm(env, runtime, &module, state, "_start", Vec::new(), None)
        .await
        .context(format!("Failed to spawn process from {}::_start()", path))?;

    let process: Process<Foo> = unsafe { Process::new(0, process.id()) };

    let tag = Tag::new();
    let mailbox = mailbox_clone.clone();
    tokio::spawn(async move {
        while let Some(msg) = receiver.recv().await {
            let foo = Foo { name: "bar".into(), process: process.clone() };
            let message = bincode::serialize(&foo).unwrap();
            let mut data_message = DataMessage::new_from_vec(None, message);
            let message: Message = Message::Data(data_message);
            mailbox.push(message);
        }
    });

    let mailbox: Mailbox<Foo> = unsafe { Mailbox::new() };
    tokio::spawn(async move {
        mailbox.tag_receive(&[tag])
        // loop { 
        //     let message = mailbox.pop(None).await ;
        //     match message {
        //         Message::Data(data) => {
        //
        //             let foo: Foo = bincode::deserialize(&data.buffer).unwrap();
        //             println!("Got a message from the module: {foo:?}");
        //         }
        //         Message::LinkDied(_) => todo!(),
        //         Message::ProcessDied(_) => todo!(),
        //     }
        // }
    });

    task.await?.unwrap();
        

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let path = "../example_module/example_module.wasm";

    // Create an unbounded channel receiver for incoming messages
    let (tx, rx) = mpsc::unbounded_channel();
    let message = WasmModuleMessage {
        name: "Example".to_string(),
        module: Vec::new(), // Your module data
        concurrency: 5,     // Your desired concurrency value
    };

    // Send the message to the receiver
    tx.send(message).expect("Failed to send message");

    run_wasm_module(path, rx).await
}
