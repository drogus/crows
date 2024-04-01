use std::time::{Duration, Instant};

// TODO: I don't really like keeping it in a shared library and not here,
// but I also don't like the idea of having to depend on the worker in WASM
// modules
use crows_shared::ConstantArrivalRateConfig;
use crate::{Runtime, InfoMessage};
use super::Executor;

pub struct ConstantArrivalRateExecutor {
    pub config: ConstantArrivalRateConfig,
    pub runtime: Runtime,
}

// TODO: k6 supports an option to set maximum number of VUs. For now
// I haven't bothered to implement any limits, but it might be useful for bigger
// tests maybe?
impl Executor for ConstantArrivalRateExecutor {
    async fn run(&mut self) -> anyhow::Result<()> {
        let update_duration = Duration::from_millis(500);
        let rate_per_second = self.config.rate as f64 / self.config.time_unit.as_secs_f64();
        let sleep_duration = Duration::from_secs_f64(1.0 / rate_per_second);

        let instant = Instant::now();
        let mut last_time_update = Instant::now();
        let mut next_run_time = instant.elapsed() + sleep_duration;
        loop {
            let handle = self.runtime.checkout_or_create_instance().await?;
            tokio::spawn(async move {
                if let Err(err) = handle.run_test().await {
                    eprintln!("An error occurred while running a scenario: {err:?}");
                }
            });

            let elapsed = instant.elapsed();
            if next_run_time > elapsed {
                if let Some(duration) = next_run_time.checked_sub(elapsed) {
                    tokio::time::sleep(duration).await;
                }
            }
            next_run_time += sleep_duration;

            // TODO: wait for all of the allocated instances finish, ie. implement
            // "graceful stop"
            if instant.elapsed() > self.config.duration {
                if let Err(err) = self.runtime.send_update(InfoMessage::Done) {
                    eprintln!("Got an error when sending an update: {err:?}");
                }
                return Ok(());
            }

            if last_time_update.elapsed() > update_duration {
                if let Err(err) = self.runtime.send_update(InfoMessage::TimingUpdate((instant.elapsed(), self.config.duration.checked_sub(instant.elapsed()).unwrap()))) {
                    eprintln!("Got an error when sending an update: {err:?}");
                }
                last_time_update = Instant::now();
            }
        }
    }

    async fn prepare(&mut self) -> anyhow::Result<()> {
        let vus = self.config.allocated_vus;
        for _ in 0..vus {
            self.runtime.reserve_instance().await?;
        }

        Ok(())
    }
}
