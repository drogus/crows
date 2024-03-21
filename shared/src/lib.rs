use std::time::Duration;

use serde::{Deserialize, Serialize};

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
            Config::ConstantArrivalRate(config) => config.split(times),
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
        // if someone uses low rate and a lot of workers we could end up
        // with rate equal to zero
        // TODO: should we worn that it's maybe not what someone wanted?
        // rate of 1 per worker may make sense if someone is just trying to test
        // requests from a lot of different locations, but if there is more workers
        // than the value of rate it feels like a mistake not a deliberate thing
        if new_config.rate == 0 {
            new_config.rate = 1;
        }

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
