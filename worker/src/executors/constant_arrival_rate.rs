use std::time::{Duration, Instant};

// TODO: I don't really like keeping it in a shared library and not here,
// but I also don't like the idea of having to depend on the worker in WASM
// modules
use crows_shared::ConstantArrivalRateConfig;
use crows_wasm::{Runtime, InfoMessage};
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
        loop {
            let handle = self.runtime.checkout_or_create_instance().await?;
            tokio::spawn(async move {
                if let Err(err) = handle.run_test().await {
                    eprintln!("An error occurred while running a scenario: {err:?}");
                }
            });
            // TODO: at the moment we always sleep for a calculated amount of time
            // This may be wrong, especially when duration is very low, because
            // with a very high request rate the time needed to spawn a task may
            // be substantial enough to delay execution. So technically we should
            // calculate how much time passed since sending the previous request and
            // only sleep for the remaining duration
            tokio::time::sleep(sleep_duration).await;

            // TODO: wait for all of the allocated instances finish, ie. implement
            // "graceful stop"
            if instant.elapsed() > self.config.duration {
                self.runtime.send_update(InfoMessage::Done);
                return Ok(());
            }

            if last_time_update.elapsed() > update_duration {
                self.runtime.send_update(InfoMessage::TimingUpdate((instant.elapsed(), self.config.duration.checked_sub(instant.elapsed()).unwrap())));
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
