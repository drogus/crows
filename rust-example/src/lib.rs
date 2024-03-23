use crows_bindings::{
    config, http_request, ConstantArrivalRateConfig, ExecutorConfig, HTTPMethod::*,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

#[config]
fn config() -> ExecutorConfig {
    let config = ConstantArrivalRateConfig {
        duration: Duration::from_secs(5),
        rate: 10,
        allocated_vus: 10,
        ..Default::default()
    };
    ExecutorConfig::ConstantArrivalRate(config)
}

#[export_name = "test"]
pub fn scenario() {
    let i: usize = rand::random();
    println!("log line from a worker, random number: {i}");
    http_request(
        "https://google.com".into(), GET, HashMap::new(), "".into(),
    );
}
