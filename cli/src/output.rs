use crows_utils::services::{IterationInfo, RequestInfo};
use crows_utils::InfoMessage;
use tokio::sync::mpsc::UnboundedReceiver;

use std::collections::HashMap;
use std::io::Stdout;
use std::io::{stdout, Write};
use std::time::Duration;

use anyhow::anyhow;
use crossterm::{
    cursor::{self, MoveTo, MoveToNextLine, MoveUp},
    execute,
    style::Print,
    terminal::{self, Clear, ClearType, ScrollUp},
};

#[derive(Default)]
pub struct SummaryStats {
    pub avg: Duration,
    pub min: Duration,
    pub med: Duration,
    pub max: Duration,
    pub p90: Duration,
    pub p95: Duration,
    pub fail_rate: f64,
    pub success_count: usize,
    pub fail_count: usize,
    pub total: usize,
}

#[derive(Default)]
pub struct WorkerState {
    pub active_instances: isize,
    pub capacity: isize,
    pub done: bool,
    pub duration: Duration,
    pub left: Duration,
    pub error: Option<String>,
}

pub trait LatencyInfo {
    fn latency(&self) -> f64;
    fn successful(&self) -> bool;
}

impl LatencyInfo for RequestInfo {
    fn latency(&self) -> f64 {
        self.latency.as_secs_f64()
    }

    fn successful(&self) -> bool {
        self.successful
    }
}

impl LatencyInfo for IterationInfo {
    fn latency(&self) -> f64 {
        self.latency.as_secs_f64()
    }

    fn successful(&self) -> bool {
        true
    }
}

pub fn print(
    stdout: &mut Stdout,
    progress_lines: u16,
    lines: Vec<String>,
    workers: &HashMap<String, WorkerState>,
    last: bool,
) -> anyhow::Result<()> {
    let (_, height) = terminal::size()?;

    execute!(
        stdout,
        MoveTo(0, height - progress_lines as u16),
        Clear(ClearType::FromCursorDown),
        MoveUp(1),
    )?;

    for line in lines {
        let (_, y) = cursor::position()?;
        execute!(stdout, Print(line))?;
        let (_, new_y) = cursor::position()?;
        let n = new_y - y;
        execute!(stdout, ScrollUp(n), MoveUp(n))?;
    }

    execute!(
        stdout,
        MoveTo(0, height - progress_lines as u16 + 1),
        Clear(ClearType::FromCursorDown),
        MoveUp(1),
    )?;

    for (name, worker) in workers {
        if worker.done {
            execute!(stdout, Print(format!("{}: Done", name,)), MoveToNextLine(1),)?;
        } else {
            let progress_percentage = worker.duration.as_secs_f64()
                / (worker.duration.as_secs_f64() + worker.left.as_secs_f64())
                * 100 as f64;
            execute!(
                stdout,
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
        execute!(stdout, Print("\n"),)?;
    }

    stdout.flush()?;

    Ok(())
}

pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let total_millis = duration.as_millis();
    let total_micros = duration.as_micros();
    let nanos = duration.subsec_nanos();

    if secs > 0 {
        format!("{:.2}s", secs as f64 + nanos as f64 / 1_000_000_000.0)
    } else if total_millis > 0 {
        format!(
            "{:.2}ms",
            total_millis as f64 + (nanos % 1_000_000) as f64 / 1_000_000.0
        )
    } else if total_micros > 0 {
        format!(
            "{:.2}µs",
            total_micros as f64 + (nanos % 1_000) as f64 / 1_000.0
        )
    } else {
        format!("{}ns", nanos)
    }
}

fn calculate_avg(latencies: &[f64]) -> f64 {
    latencies.iter().sum::<f64>() / latencies.len() as f64
}

fn calculate_min(latencies: &[f64]) -> f64 {
    *latencies
        .iter()
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap()
}

fn calculate_max(latencies: &[f64]) -> f64 {
    *latencies
        .iter()
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap()
}

fn calculate_percentile(latencies: &[f64], percentile: f64) -> f64 {
    let idx = (percentile / 100.0 * latencies.len() as f64).ceil() as usize - 1;
    latencies[idx]
}

fn calculate_median(latencies: &[f64]) -> f64 {
    let mid = latencies.len() / 2;
    if latencies.len() % 2 == 0 {
        (latencies[mid - 1] + latencies[mid]) / 2.0
    } else {
        latencies[mid]
    }
}

pub fn calculate_summary<T>(latencies: &Vec<T>) -> SummaryStats
where
    T: LatencyInfo,
{
    let mut latencies_sorted: Vec<f64> = latencies.iter().map(|l| l.latency()).collect();
    latencies_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let fail_count = latencies.iter().filter(|l| !l.successful()).count();
    let success_count = latencies.iter().filter(|l| l.successful()).count();
    let fail_rate = fail_count as f64 / latencies.len() as f64 * 100f64;

    SummaryStats {
        avg: Duration::from_secs_f64(calculate_avg(&latencies_sorted)),
        min: Duration::from_secs_f64(calculate_min(&latencies_sorted)),
        max: Duration::from_secs_f64(calculate_max(&latencies_sorted)),
        med: Duration::from_secs_f64(calculate_median(&latencies_sorted)),
        p90: Duration::from_secs_f64(calculate_percentile(&latencies_sorted, 90.0)),
        p95: Duration::from_secs_f64(calculate_percentile(&latencies_sorted, 95.0)),
        total: latencies.len(),
        fail_rate,
        success_count,
        fail_count,
    }
}

pub async fn drive_progress(
    worker_names: Vec<String>,
    mut updates_receiver: UnboundedReceiver<(String, InfoMessage)>,
) -> anyhow::Result<()> {
    let mut stdout = stdout();

    let progress_lines = worker_names.len() as u16;

    let mut all_request_stats: Vec<RequestInfo> = Vec::new();
    let mut all_iteration_stats: Vec<IterationInfo> = Vec::new();
    let mut worker_states: HashMap<String, WorkerState> = HashMap::new();

    for name in worker_names {
        worker_states.insert(name.clone(), Default::default());
    }

    while let Some((worker_name, update)) = updates_receiver.recv().await {
        let mut lines = Vec::new();
        let state = worker_states
            .get_mut(&worker_name)
            .ok_or(anyhow!("Couldn't findt the worker"))?;

        match update {
            InfoMessage::Stderr(buf) => {
                lines.push(format!(
                    "[ERROR][{worker_name}] {}",
                    String::from_utf8_lossy(&buf)
                ));
            }
            InfoMessage::Stdout(buf) => {
                lines.push(format!(
                    "[INFO][{worker_name}] {}",
                    String::from_utf8_lossy(&buf)
                ));
            }
            InfoMessage::RequestInfo(info) => {
                all_request_stats.push(info);
            }
            InfoMessage::IterationInfo(info) => {
                all_iteration_stats.push(info);
            }
            InfoMessage::InstanceCheckedOut => {
                state.active_instances += 1;
            }
            InfoMessage::InstanceReserved => {
                state.capacity += 1;
            }
            InfoMessage::InstanceCheckedIn => {
                state.active_instances -= 1;
            }
            InfoMessage::TimingUpdate((elapsed, left)) => {
                state.duration = elapsed;
                state.left = left;
            }
            InfoMessage::Done => state.done = true,
            InfoMessage::PrepareError(message) => state.error = Some(message),
            InfoMessage::RunError(message) => state.error = Some(message),
        }

        if worker_states.values().all(|s| s.done) {
            break;
        }

        print(&mut stdout, progress_lines, lines, &worker_states, false).unwrap();
    }

    let request_summary = calculate_summary(&all_request_stats);
    let iteration_summary = calculate_summary(&all_iteration_stats);

    let mut lines = Vec::new();
    lines.push(format!("\n\nSummary:\n"));
    lines.push(format!(
        "http_req_duration..........: avg={}\tmin={}\tmed={}\tmax={}\tp(90)={}\tp(95)={}\n",
        format_duration(request_summary.avg),
        format_duration(request_summary.min),
        format_duration(request_summary.med),
        format_duration(request_summary.max),
        format_duration(request_summary.p90),
        format_duration(request_summary.p95)
    ));
    lines.push(format!(
        "http_req_failed............: {:.2}%\t✓ {}\t✗ {}\n",
        request_summary.fail_rate, request_summary.success_count, request_summary.fail_count
    ));
    lines.push(format!(
        "http_reqs..................: {}\n",
        request_summary.total
    ));
    lines.push(format!(
        "iteration_duration.........: avg={}\tmin={}\tmed={}\tmax={}\tp(90)={}\tp(95)={}\n",
        format_duration(iteration_summary.avg),
        format_duration(iteration_summary.min),
        format_duration(iteration_summary.med),
        format_duration(iteration_summary.max),
        format_duration(iteration_summary.p90),
        format_duration(iteration_summary.p95)
    ));
    lines.push(format!(
        "iterations.................: {}\n",
        iteration_summary.total
    ));
    lines.push(format!("\n\n"));

    print(&mut stdout, progress_lines, lines, &worker_states, true)?;

    Ok(())
}
