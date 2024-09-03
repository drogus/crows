use crows_bindings::{
    config, http_request, ConstantArrivalRateConfig, ExecutorConfig, HTTPMethod::*,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

#[config]
fn config() -> ExecutorConfig {
    let config = ConstantArrivalRateConfig {
        duration: Duration::from_secs(1),
        rate: 20,
        allocated_vus: 1,
        ..Default::default()
    };
    ExecutorConfig::ConstantArrivalRate(config)
}

#[export_name = "scenario"]
pub fn scenario() {
    let server_url = std::env::var("SERVER_URL").unwrap();
    let _ = http_request(
        server_url, GET, HashMap::new(), "".into(),
    );
}
