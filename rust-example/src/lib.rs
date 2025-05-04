// Use wit_bindgen to generate the bindings from the component model to Rust.
// For more information see: https://github.com/bytecodealliance/wit-bindgen/
wit_bindgen::generate!({
    path: "../wasm",
    world: "crows",
});

use std::collections::HashMap;
use std::time::Duration;

struct GuestComponent;

export!(GuestComponent);

use crate::local::crows::types::{ConstantArrivalRateConfig, HttpMethod, Request};

impl Guest for GuestComponent {
    fn get_config() -> Config {
        let config = ConstantArrivalRateConfig {
            duration: 10000,
            rate: 1,
            allocated_vus: 1,
            graceful_shutdown_timeout: 1000,
            time_unit: 1000,
        };
        Config::ConstantArrivalRate(config)
    }

    fn run_scenario() {
        let server_url = std::env::var("SERVER_URL").unwrap();
        let request = Request {
            uri: server_url,
            method: HttpMethod::Get,
            headers: Vec::new(),
            body: None,
        };
        let _ = host::http_request(&request);
    }
}
