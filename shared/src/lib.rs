use std::time::Duration;

use serde::{Serialize, Deserialize};

pub trait ExecutorConfig {
    fn split(&self, times: usize) -> Config;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Config {
    ConstantArrivalRate(ConstantArrivalRateConfig),
}

impl Config {
    pub fn split(&self, times: usize) -> Config {
        match self {
            Config::ConstantArrivalRate(config) => config.split(times)
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConstantArrivalRateConfig {
    pub duration: Duration,
    pub rate: usize,
    pub time_unit: Duration,
    pub allocated_vus: usize,
}

impl ExecutorConfig for ConstantArrivalRateConfig {
    fn split(&self, times: usize) -> Config {
        let mut new_config = self.clone();

        new_config.rate /= times;

        Config::ConstantArrivalRate(new_config)
    }
}

impl Default for ConstantArrivalRateConfig {
    fn default() -> Self {
        Self {
            duration: Default::default(),
            rate: Default::default(),
            time_unit: Duration::from_secs(1),
            allocated_vus: Default::default(),
        }
    }
}
