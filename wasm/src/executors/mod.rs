mod constant_arrival_rate;
use crate::Runtime;
use constant_arrival_rate::ConstantArrivalRateExecutor;
use crows_shared::Config;

pub trait Executor {
    #[allow(async_fn_in_trait)]
    async fn prepare(&mut self) -> anyhow::Result<()>;
    #[allow(async_fn_in_trait)]
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

    pub async fn run(&mut self) -> anyhow::Result<()> {
        match self {
            Executors::ConstantArrivalRateExecutor(ref mut executor) => executor.run().await?,
        }

        Ok(())
    }

    pub async fn prepare(&mut self) -> anyhow::Result<()> {
        match self {
            Executors::ConstantArrivalRateExecutor(ref mut executor) => executor.prepare().await?,
        }

        Ok(())
    }
}
