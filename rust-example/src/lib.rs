use std::cell::RefCell;
use std::collections::HashMap;
use crows_bindings::{http_request, HTTPMethod::*};

#[export_name="test"]
pub fn test() {
    let response = http_request("foo".into(), GET, HashMap::new(), "".into());
    println!("response: {response:?}");
}


