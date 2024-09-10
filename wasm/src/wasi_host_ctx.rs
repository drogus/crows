use anyhow::{anyhow, Result};
use crows_bindings::{HTTPError, HTTPRequest, HTTPResponse};
use crows_utils::services::RequestInfo;
use futures::Future;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{from_slice, to_vec};
use std::pin::Pin;
use tokio::sync::mpsc::UnboundedSender;
use wasmtime::{Caller, Memory};
use wasmtime_wasi::preview1::WasiPreview1Adapter;

use crate::get_memory;
use crate::http_client::Client;

pub struct WasiHostCtx {
    pub preview2_ctx: wasmtime_wasi::WasiCtx,
    pub preview2_table: wasmtime::component::ResourceTable,
    pub preview1_adapter: WasiPreview1Adapter,
    pub memory: Option<Memory>,
    pub buffers: slab::Slab<Box<[u8]>>,
    pub client: Client,
    pub request_info_sender: UnboundedSender<RequestInfo>,
    pub stderr_sender: UnboundedSender<Vec<u8>>,
}

fn create_return_value(status: u8, length: u32, ptr: u32) -> u64 {
    assert!(
        length <= 0x00FFFFFF,
        "Length must be no larger than 3 bytes"
    );
    ((status as u64) << 56) | ((length as u64) << 32) | (ptr as u64)
}

impl WasiHostCtx {
    pub fn instantiate(&mut self, mem: Memory) {
        self.memory = Some(mem);
    }

    pub async fn wrap_async<'a, T, U, F, E>(
        mut caller: Caller<'a, Self>,
        ptr: u32,
        len: u32,
        f: F,
    ) -> Result<u64>
    where
        F: for<'b> FnOnce(
            &'b mut Caller<'_, Self>,
            T,
        ) -> Pin<Box<dyn Future<Output = Result<U, E>> + 'b + Send>>,
        U: Serialize,
        E: Serialize,
        T: DeserializeOwned,
    {
        let memory = get_memory(&mut caller)?;

        let slice = memory
            .data(&caller)
            .get(ptr as usize..(ptr + len) as usize)
            .ok_or(anyhow!("Could not get memory slice"))?;

        let arg = from_slice(slice)?;

        let result = f(&mut caller, arg).await;

        let (_, store) = { memory.data_and_store_mut(&mut caller) };

        match result {
            Ok(ret) => {
                let encoded = to_vec(&ret)?;

                let length = encoded.len();
                let index = store.buffers.insert(encoded.into_boxed_slice());

                Ok(create_return_value(0, length as u32, index as u32))
            }
            Err(err) => {
                let encoded = to_vec(&err)?;

                let length = encoded.len();
                let index = store.buffers.insert(encoded.into_boxed_slice());

                Ok(create_return_value(1, length as u32, index as u32))
            }
        }
    }

    pub fn http<'a>(
        mut caller: &'a mut Caller<'_, Self>,
        request: HTTPRequest,
    ) -> Pin<Box<dyn Future<Output = Result<HTTPResponse, HTTPError>> + 'a + Send>> {
        Box::pin(async move {
            let memory = get_memory(&mut caller).unwrap();
            let (_, store) = memory.data_and_store_mut(&mut caller);

            let (http_response, request_info) = store.client.http_request(request).await.map_err(|e| { println!("Error: {e:?}"); e})?;

            let _ = store.request_info_sender.send(request_info);

            Ok(http_response)
        })
    }

    pub fn set_config(mut caller: Caller<'_, Self>, ptr: u32, len: u32) -> Result<u32> {
        let memory = get_memory(&mut caller)?;

        let slice = memory
            .data(&caller)
            .get(ptr as usize..(ptr + len) as usize)
            .ok_or(anyhow!("Could not get memory slice"))?
            .to_owned()
            .into_boxed_slice();

        let (_, store) = memory.data_and_store_mut(&mut caller);

        let index = store.buffers.insert(slice);

        Ok(index as u32)
    }

    pub fn consume_buffer(
        mut caller: Caller<'_, Self>,
        index: u32,
        ptr: u32,
        len: u32,
    ) -> Result<()> {
        let memory = get_memory(&mut caller)?;
        let (slice, store) = memory.data_and_store_mut(&mut caller);

        let buffer = store
            .buffers
            .try_remove(index as usize)
            .ok_or(anyhow!("Could not remove slab buffer"))?;

        anyhow::ensure!(
            len as usize == buffer.len(),
            "bad length passed to consume_buffer"
        );

        slice
            .get_mut(ptr as usize..)
            .and_then(|s| s.get_mut(..len as usize))
            .ok_or(anyhow!("Could not fetch slice from WASM memory"))?
            .copy_from_slice(&buffer);

        Ok(())
    }
}

impl wasmtime_wasi::WasiView for WasiHostCtx {
    fn table(&mut self) -> &mut wasmtime::component::ResourceTable {
        &mut self.preview2_table
    }

    fn ctx(&mut self) -> &mut wasmtime_wasi::WasiCtx {
        &mut self.preview2_ctx
    }
}

impl wasmtime_wasi::preview1::WasiPreview1View for WasiHostCtx {
    fn adapter(&self) -> &WasiPreview1Adapter {
        &self.preview1_adapter
    }

    fn adapter_mut(&mut self) -> &mut WasiPreview1Adapter {
        &mut self.preview1_adapter
    }
}
