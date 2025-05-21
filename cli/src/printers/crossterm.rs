use std::collections::HashMap;
use std::io::{stdout, Stdout, Write};

use anyhow::Result;
use crossterm::{
    cursor::{self, MoveDown, MoveTo, MoveToNextLine, MoveUp},
    execute,
    style::Print,
    terminal::{self, Clear, ClearType, ScrollDown, ScrollUp},
};

use crate::output::{OutputPrinter, WorkerState};

/// CrosstermPrinter implements the OutputPrinter trait using crossterm for advanced terminal output
/// with progress bars and dynamic updates.
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
        last: bool,
    ) -> Result<()> {
        let progress_lines = self.workers_count as u16;
        let (_, height) = terminal::size()?;

        execute!(
            self.stdout,
            MoveTo(0, height - progress_lines - 1 as u16),
            Clear(ClearType::CurrentLine),
            Clear(ClearType::FromCursorDown),
        )?;

        for line in lines {
            let (_, y) = cursor::position()?;
            execute!(self.stdout, Print(line))?;
            let (_, new_y) = cursor::position()?;
            let n = new_y - y;
            execute!(self.stdout, ScrollUp(n + 1), MoveToNextLine(n + 1))?;
        }

        execute!(
            self.stdout,
            MoveTo(0, height - progress_lines + 1 as u16),
            Clear(ClearType::FromCursorDown),
        )?;

        for (name, worker) in workers {
            // TODO: this should really be an enum
            if let Some(_) = worker.error {
                execute!(
                    self.stdout,
                    Print(format!("{}: Error", name,)),
                    MoveToNextLine(1),
                )?;
            } else if worker.done {
                execute!(self.stdout, Print(format!("{}: Done", name,)), MoveToNextLine(1),)?;
            } else {
                let progress_percentage = worker.duration.as_secs_f64()
                    / (worker.duration.as_secs_f64() + worker.left.as_secs_f64())
                    * 100 as f64;
                execute!(
                    self.stdout,
                    Print(format!(
                        "{}: [{: <25}] {:.2}% ({}/{})",
                        name,
                        "*".repeat((progress_percentage as usize) / 4),
                        progress_percentage,
                        worker.active_instances,
                        worker.capacity,
                    )),
                    MoveToNextLine(1),
                )?;
            }
        }
        if last {
            execute!(self.stdout, Print("\n"),)?;
        }

        self.stdout.flush()?;

        Ok(())
    }

    fn print_workers_summary(
        &mut self,
        workers: &HashMap<String, WorkerState>,
    ) -> Result<()> {
        let (_, height) = terminal::size()?;

        execute!(
            self.stdout,
            MoveTo(0, height - workers.len() as u16 + 1),
            Clear(ClearType::FromCursorDown),
            MoveUp(1),
        )?;

        for (name, worker) in workers {
            if let Some(ref msg) = worker.error {
                execute!(self.stdout, Print(format!("{}: Error - {msg}\n", name,)),)?;
            } else if worker.done {
                execute!(self.stdout, Print(format!("{}: Done\n", name,)),)?;
            }
        }

        self.stdout.flush()?;

        Ok(())
    }

    fn initialize(&mut self, workers_count: usize) -> Result<()> {
        self.workers_count = workers_count;
        let progress_lines = workers_count as u16;
        
        execute!(
            self.stdout,
            ScrollUp(progress_lines),
            MoveUp(progress_lines),
            Clear(ClearType::FromCursorDown),
        )?;
        
        Ok(())
    }
}
