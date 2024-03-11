use std::cell::RefCell;
use std::collections::HashMap;
use crows_bindings::{http_request, HTTPMethod::*};

#[export_name="test"]
pub fn test() {
    let response = http_request("https://example.com".into(), GET, HashMap::new(), "".into());
    println!("response: {:?}", response.unwrap().status);
}
