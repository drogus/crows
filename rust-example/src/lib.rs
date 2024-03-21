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
        rate: 1,
        time_unit: Duration::from_secs(1),
        allocated_vus: 1,
        ..Default::default()
    };
    ExecutorConfig::ConstantArrivalRate(config)
}

#[export_name = "test"]
pub fn test() {
    let i: usize = rand::random();
    println!("log line from a worker, random number: {i}");
    let response = http_request(
        "http://127.0.0.1:8080/".into(),
        GET,
        HashMap::new(),
        "".into(),
    );
    let response = http_request(
        "http://127.0.0.1:8080/".into(),
        GET,
        HashMap::new(),
        "".into(),
    );
     let response = http_request(
        "http://127.0.0.1:8080/".into(),
        GET,
        HashMap::new(),
        "".into(),
    );
    let i: usize = rand::random();
}
