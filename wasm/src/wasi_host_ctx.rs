use tokio::sync::mpsc::UnboundedSender;
use wasmtime::Memory;
use wasmtime_wasi::{WasiCtx, WasiView};

use crate::runtime::HostComponent;

pub struct WasiHostCtx {
    pub host: HostComponent,
    pub wasi: WasiCtx,
    pub table: wasmtime::component::ResourceTable,
    pub memory: Option<Memory>,
    pub stderr_sender: UnboundedSender<Vec<u8>>,
}

impl WasiHostCtx {
    pub fn instantiate(&mut self, mem: Memory) {
        self.memory = Some(mem);
    }
}

impl WasiView for WasiHostCtx {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
    fn table(&mut self) -> &mut wasmtime::component::ResourceTable {
        &mut self.table
    }
}
