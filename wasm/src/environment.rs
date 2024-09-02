use anyhow::Result;
use wasmtime::{Engine, Linker};
use crate::WasiHostCtx;

#[derive(Clone)]
pub struct Environment {
    pub engine: Engine,
    pub linker: Linker<WasiHostCtx>,
}

impl Environment {
    pub fn new() -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.async_support(true);
        config.consume_fuel(true);

        let engine = Engine::new(&config)?;

        let mut linker = Linker::new(&engine);

        linker
            .func_wrap("crows", "consume_buffer", WasiHostCtx::consume_buffer)
            .unwrap();
        linker
            .func_wrap2_async("crows", "http", |caller, ptr, len| {
                Box::new(async move {
                    WasiHostCtx::wrap_async(caller, ptr, len, WasiHostCtx::http).await
                })
            })
            .unwrap();
        linker
            .func_wrap("crows", "set_config", WasiHostCtx::set_config)
            .unwrap();

        wasmtime_wasi::preview1::add_to_linker_async(&mut linker, |t| t)?;

        Ok(Self { engine, linker })
    }
}
