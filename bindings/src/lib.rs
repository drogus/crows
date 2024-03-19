use std::{cell::RefCell, collections::HashMap};

pub use crows_macros::config;
use serde::{Serialize, Deserialize, de::DeserializeOwned};
use serde_json::{to_vec, from_slice};
pub use crows_shared::Config as ExecutorConfig;
pub use crows_shared::ConstantArrivalRateConfig;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum HTTPMethod {
    HEAD,
    GET,
    POST,
    PUT,
    DELETE,
    OPTIONS,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct HTTPRequest {
    // TODO: these should not be public I think, I'd prefer to do a public interface for them
    pub url: String,
    pub method: HTTPMethod,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HTTPError {
    pub message: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct HTTPResponse {
    // TODO: these should not be public I think, I'd prefer to do a public interface for them
    pub headers: HashMap<String, String>,
    pub body: String,
    pub status: u16,
}

fn extract_from_return_value(value: u64) -> (u8, u32, u32) {
    let status = ((value >> 56) & 0xFF) as u8;
    let length = ((value >> 32) & 0x00FFFFFF) as u32;
    let ptr = (value & 0xFFFFFFFF) as u32;
    (status, length, ptr)
}

mod bindings {
    #[link(wasm_import_module = "crows")]
    extern "C" {
        pub fn log(content: *mut u8, content_len: usize);
        pub fn http(content: *mut u8, content_len: usize) -> u64;
        pub fn consume_buffer(index: u32, content: *mut u8, content_len: usize);
        pub fn set_config(content: *mut u8, content_len: usize) -> u32;
    }
}

fn with_buffer<R>(f: impl FnOnce(&mut Vec<u8>) -> R) -> R {
    // using a buffer saved in thread_local allows us to share it between function calls
    thread_local! {
        static BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::with_capacity(1024));
    }

    BUFFER.with(|r| {
        let mut buf = r.borrow_mut();
        buf.clear();
        f(&mut buf)
    })
}

pub fn http_request(
    url: String,
    method: HTTPMethod,
    headers: HashMap<String, String>,
    body: String,
) -> Result<HTTPResponse, HTTPError> {
    let body = Some(body);
    let request = HTTPRequest {
        method,
        url,
        headers,
        body,
    };

    call_host_function(&request, |buf| unsafe {
        bindings::http(buf.as_mut_ptr(), buf.len())
    })
}

fn call_host_function<T, R, E>(arguments: &T, f: impl FnOnce(&mut Vec<u8>) -> u64) -> Result<R, E>
where
    T: Serialize,
    R: DeserializeOwned,
    E: DeserializeOwned,
{
    let mut encoded = to_vec(arguments).unwrap();

    let response = f(&mut encoded);

    let (status, length, index) = extract_from_return_value(response);

    let mut buf = encoded;
    buf.clear();

    // when using reserve_exact it guarantees capacity to be vector.len() + additional long,
    // thus we can just use length for reserving
    buf.reserve_exact(length as usize);

    unsafe {
        bindings::consume_buffer(index, buf.as_mut_ptr(), length as usize);
        buf.set_len(length as usize);
    }

    if status == 0 {
        Ok(from_slice(&buf).expect("Couldn't decode message from the host"))
    } else {
        Err(from_slice(&buf).expect("Couldn't decode message from the host"))
    }
}

pub fn __set_config(config: ExecutorConfig) -> u32 {
    let mut encoded = to_vec(&config).unwrap();
    unsafe { bindings::set_config(encoded.as_mut_ptr(), encoded.len()) }
}
