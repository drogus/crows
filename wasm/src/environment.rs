use crate::{runtime::host, WasiHostCtx};
use anyhow::Result;
use wasmtime::component::{HasSelf, Linker};
use wasmtime::Engine;

#[derive(Clone)]
pub struct Environment {
    pub engine: Engine,
    pub linker: Linker<WasiHostCtx>,
}

impl Environment {
    pub fn new() -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);
        config.async_support(true);
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;

        let mut linker = Linker::new(&engine);

        host::add_to_linker::<_, HasSelf<_>>(&mut linker, |state: &mut WasiHostCtx| &mut state.host)?;

        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;

        Ok(Self { engine, linker })
    }
}
