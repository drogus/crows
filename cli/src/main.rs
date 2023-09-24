#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

use std::time::Duration;

use tokio::time::sleep;
use utils::services::connect_to_coordinator;
use utils::services::{Client, Coordinator};

struct ClientService;

impl Client for ClientService {
}

#[tokio::main]
pub async fn main() {
    let service = ClientService {};
    let coordinator = connect_to_coordinator("127.0.0.1:8282", service)
        .await
        .unwrap();

    loop {
        let response = coordinator.upload_scenario("Foo".into(), Vec::new()).await;
        println!("Response from coordinator: {response:?}");
        sleep(Duration::from_millis(1000)).await;
    }
}
