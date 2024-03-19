use std::time::Instant;

use crows_wasm::{run_wasm, Instance, Runtime};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), anyhow::Error> {
    // let path = std::env::var("MODULE_PATH").expect("MODULE_PATH env var is not set");
    // let content = std::fs::read(path).unwrap();
    //
    // let mut receivers = Vec::new();
    // let runtime = Runtime::new()?;
    // let instant = Instant::now();
    // let mut futures = Vec::new();
    //
    // // let (mut instance, receiver) = Instance::new(&content, &runtime.environment).await.unwrap();
    // // receivers.push(receiver);
    // // let config = crows_wasm::fetch_config(&mut instance).await.unwrap();
    // // println!("Config: {config:?}");
    // for _ in 0..10 {
    //     let (mut instance, receiver) = Instance::new(&content, &runtime.environment).await.unwrap();
    //     receivers.push(receiver);
    //     let fut = async move {
    //         run_wasm(&mut instance).await.unwrap();
    //     };
    //     futures.push(fut);
    // }
    //
    // tokio::spawn(async move {
    //     let mut futures = Vec::new();
    //     for mut receiver in receivers {
    //         let fut = async move {
    //             while let Some(message) = receiver.recv().await {
    //                 println!("stdout: {}", String::from_utf8(message).unwrap());
    //             }
    //         };
    //         futures.push(fut);
    //     }
    //
    //     futures::future::join_all(futures).await;
    // });
    //
    // futures::future::join_all(futures).await;
    //
    // println!("elapsed: {}ms", instant.elapsed().as_millis());
    //
    Ok(())
}
