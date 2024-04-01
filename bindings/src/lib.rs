use std::{cell::RefCell, collections::HashMap};

pub use crows_macros::config;
pub use crows_shared::Config as ExecutorConfig;
pub use crows_shared::ConstantArrivalRateConfig;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::to_writer;
use serde_json::{from_slice, to_vec};

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
    with_buffer(|mut buf| {
        to_writer(&mut buf, arguments).unwrap();

        let response = f(buf);

        let (status, length, index) = extract_from_return_value(response);

        buf.try_reserve_exact(length as usize).unwrap();

        consume_buffer(index, buf, length as usize);

        if status == 0 {
            Ok(from_slice(&buf).expect("Couldn't decode message from the host"))
        } else {
            Err(from_slice(&buf).expect("Couldn't decode message from the host"))
        }
    })
}

fn consume_buffer(index: u32, buf: &mut Vec<u8>, content_len: usize) {
    unsafe {
        bindings::consume_buffer(index, buf.as_mut_ptr(), content_len as usize);
        buf.set_len(content_len as usize)
    }
}

pub fn __set_config(config: ExecutorConfig) -> u32 {
    let mut encoded = to_vec(&config).unwrap();
    unsafe { bindings::set_config(encoded.as_mut_ptr(), encoded.len()) }
}
