mod constant_arrival_rate;
use constant_arrival_rate::ConstantArrivalRateExecutor;
use crows_shared::Config;
use crows_wasm::Runtime;

pub trait Executor {
    async fn prepare(&mut self) -> anyhow::Result<()>;
    async fn run(&mut self) -> anyhow::Result<()>;
}

pub enum Executors {
    ConstantArrivalRateExecutor(ConstantArrivalRateExecutor),
}

impl Executors {
    pub async fn create_executor(config: Config, runtime: Runtime) -> Self {
        match config {
            Config::ConstantArrivalRate(config) => {
                Executors::ConstantArrivalRateExecutor(ConstantArrivalRateExecutor {
                    config,
                    runtime,
                })
            }
        }
    }

    pub async fn run(&mut self) {
        match self {
            Executors::ConstantArrivalRateExecutor(ref mut executor) => {
                executor.run().await.unwrap()
            }
        }
    }

    pub async fn prepare(&mut self) {
        match self {
            Executors::ConstantArrivalRateExecutor(ref mut executor) => {
                executor.prepare().await.unwrap()
            }
        }
    }
}
