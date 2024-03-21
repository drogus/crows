use crows_utils::services::{CoordinatorClient, IterationInfo, RequestInfo};

use std::collections::HashMap;
use std::io::Stdout;
use std::io::{stdout, Write};
use std::time::Duration;

use anyhow::anyhow;
use crossterm::{
    cursor::{self, MoveTo, MoveToNextLine, MoveUp, RestorePosition, SavePosition},
    execute,
    style::Print,
    terminal::{self, Clear, ClearType, ScrollUp},
};

#[derive(Default)]
struct WorkerState {
    active_instances: isize,
    capacity: isize,
    done: bool,
}

pub async fn start(
    coordinator: &CoordinatorClient,
    name: &str,
    workers_number: &usize,
) -> anyhow::Result<()> {
    let (run_id, mut worker_names) = coordinator
        .start(name.to_string(), workers_number.clone())
        .await
        .unwrap()
        .unwrap();

    worker_names.sort();
    println!("Worker names: {worker_names:?}");

    let mut stdout = stdout();

    let progress_lines = worker_names.len() as u16;

    let mut all_request_stats: Vec<RequestInfo> = Vec::new();
    let mut all_iteration_stats: Vec<IterationInfo> = Vec::new();
    let mut worker_states: HashMap<String, WorkerState> = HashMap::new();
    let mut bars = HashMap::new();

    for name in worker_names {
        worker_states.insert(name.clone(), Default::default());

        bars.insert(
            name.clone(),
            BarData {
                worker_name: name.clone(),
                left: Duration::from_secs(1),
                ..Default::default()
            },
        );
    }

    loop {
        let mut lines = Vec::new();
        let result = coordinator.get_run_status(run_id.clone()).await.unwrap();

        if worker_states.values().all(|s| s.done) {
            break;
        }

        for (worker_name, run_info) in result.unwrap().iter() {
            let state = worker_states
                .get_mut(worker_name)
                .ok_or(anyhow!("Couldn't findt the worker"))?;
            state.active_instances += run_info.active_instances_delta;
            state.capacity += run_info.capacity_delta;

            all_request_stats.extend(run_info.request_stats.clone());
            all_iteration_stats.extend(run_info.iteration_stats.clone());

            for log_line in &run_info.stdout {
                lines.push(format!(
                    "[INFO][{worker_name}] {}",
                    String::from_utf8_lossy(log_line)
                ));
            }
            for log_line in &run_info.stderr {
                lines.push(format!(
                    "[ERROR][{worker_name}] {}",
                    String::from_utf8_lossy(log_line)
                ));
            }

            if run_info.done {
                state.done = true;
            }

            let bar = bars
                .get_mut(worker_name)
                .ok_or(anyhow!("Couldn't find bar data for worker {worker_name}"))?;
            bar.active_vus = state.active_instances as usize;
            bar.all_vus = state.capacity as usize;
            if let Some(duration) = run_info.elapsed {
                bar.duration = duration;
            }
            if let Some(left) = run_info.left {
                bar.left = left;
            }
            bar.done = state.done;
        }

        print(&mut stdout, progress_lines, lines, &bars, false).unwrap();
        tokio::time::sleep(Duration::from_millis(250)).await;
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

    print(&mut stdout, progress_lines, lines, &bars, true)?;

    Ok(())
}

trait LatencyInfo {
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

fn calculate_summary<T>(latencies: &Vec<T>) -> SummaryStats
where
    T: LatencyInfo,
{
    let mut latencies_sorted: Vec<f64> = latencies.iter().map(|l| l.latency()).collect();
    latencies_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let fail_count = latencies.iter().filter(|l| !l.successful()).count();
    let success_count = latencies.iter().filter(|l| l.successful()).count();
    let fail_rate = fail_count as f64 / latencies.len() as f64;

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

#[derive(Default)]
struct SummaryStats {
    avg: Duration,
    min: Duration,
    med: Duration,
    max: Duration,
    p90: Duration,
    p95: Duration,
    fail_rate: f64,
    success_count: usize,
    fail_count: usize,
    total: usize,
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

fn format_duration(duration: Duration) -> String {
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

fn print(
    stdout: &mut Stdout,
    progress_lines: u16,
    lines: Vec<String>,
    bars: &HashMap<String, BarData>,
    last: bool,
) -> anyhow::Result<()> {
    let (_, height) = terminal::size()?;

    execute!(
        stdout,
        MoveTo(0, height - progress_lines as u16),
        Clear(ClearType::FromCursorDown),
    )?;

    execute!(stdout, RestorePosition)?;

    for line in lines {
        execute!(stdout, Print(line),)?;
    }

    let (_, y) = cursor::position()?;
    if y > height - progress_lines {
        let n = y - (height - progress_lines);
        execute!(stdout, ScrollUp(n), MoveUp(n))?;
    }
    execute!(stdout, SavePosition)?;

    execute!(
        stdout,
        MoveTo(0, height - progress_lines as u16 + 1),
        Clear(ClearType::FromCursorDown),
        MoveUp(1),
    )?;

    for (_, bar) in bars {
        if bar.done {
            execute!(
                stdout,
                Print(format!("{}: Done", bar.worker_name,)),
                MoveToNextLine(1),
            )?;
        } else {
            let progress_percentage = bar.duration.as_secs_f64()
                / (bar.duration.as_secs_f64() + bar.left.as_secs_f64())
                * 100 as f64;
            execute!(
                stdout,
                Print(format!(
                    "{}: [{: <25}] {:.2}% ({}/{})",
                    bar.worker_name,
                    "*".repeat((progress_percentage as usize) / 4),
                    progress_percentage,
                    bar.active_vus,
                    bar.all_vus,
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

#[derive(Default)]
struct BarData {
    worker_name: String,
    active_vus: usize,
    all_vus: usize,
    duration: Duration,
    left: Duration,
    done: bool,
}
