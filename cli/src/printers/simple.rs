use std::collections::HashMap;
use std::io::{stdout, Stdout, Write};

use anyhow::Result;

use crate::output::{OutputPrinter, WorkerState};

/// SimplePrinter implements the OutputPrinter trait using simple text output
/// without any terminal manipulation, making it ideal for debugging.
pub struct Printer {
    stdout: Stdout,
    workers_count: usize,
}

impl Default for Printer {
    fn default() -> Self {
        Self {
            stdout: stdout(),
            workers_count: 0,
        }
    }
}

impl OutputPrinter for Printer {
    fn print(
        &mut self,
        lines: Vec<String>,
        workers: &HashMap<String, WorkerState>,
        _last: bool,
    ) -> Result<()> {
        // Print all lines as they come
        for line in lines {
            writeln!(self.stdout, "{}", line)?;
        }

        // Print simple progress information for each worker
        for (name, worker) in workers {
            if let Some(ref error) = worker.error {
                writeln!(self.stdout, "[PROGRESS] {}: Error - {}", name, error)?;
            } else if worker.done {
                writeln!(self.stdout, "[PROGRESS] {}: Done", name)?;
            } else {
                let progress_percentage = worker.duration.as_secs_f64()
                    / (worker.duration.as_secs_f64() + worker.left.as_secs_f64())
                    * 100 as f64;
                writeln!(
                    self.stdout,
                    "[PROGRESS] {}: {:.2}% ({}/{})",
                    name,
                    progress_percentage,
                    worker.active_instances,
                    worker.capacity,
                )?;
            }
        }

        self.stdout.flush()?;

        Ok(())
    }

    fn print_workers_summary(
        &mut self,
        workers: &HashMap<String, WorkerState>,
    ) -> Result<()> {
        writeln!(self.stdout, "\n[SUMMARY] Worker Results:")?;
        
        for (name, worker) in workers {
            if let Some(ref msg) = worker.error {
                writeln!(self.stdout, "[SUMMARY] {}: Error - {}", name, msg)?;
            } else if worker.done {
                writeln!(self.stdout, "[SUMMARY] {}: Done", name)?;
            }
        }

        self.stdout.flush()?;

        Ok(())
    }

    fn initialize(&mut self, workers_count: usize) -> Result<()> {
        self.workers_count = workers_count;
        Ok(())
    }
}
