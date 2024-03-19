use crows_bindings::{
    config, http_request, ConstantArrivalRateConfig, ExecutorConfig, HTTPMethod::*,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

#[config]
fn config() -> ExecutorConfig {
    let config = ConstantArrivalRateConfig {
        duration: Duration::from_secs(10),
        rate: 10,
        time_unit: Duration::from_secs(1),
        allocated_vus: 10,
        ..Default::default()
    };
    ExecutorConfig::ConstantArrivalRate(config)
}

#[export_name = "test"]
pub fn test() {
    let response = http_request("http://127.0.0.1:8080/".into(), GET, HashMap::new(), "".into());
    // println!("response: {:?}", response.unwrap().status);
}
