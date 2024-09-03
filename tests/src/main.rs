use anyhow::Result;
use crows_shared::{Config, ConstantArrivalRateConfig};
use crows_utils::services::{connect_to_coordinator, CoordinatorClient};
use crows_utils::InfoMessage;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() {
    println!("Running tests...");
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

    use super::*;

    async fn start_http_server(request_count: Arc<Mutex<usize>>) -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:8080").await?;
        println!("HTTP server listening on {}", listener.local_addr()?);

        loop {
            let (mut stream, _) = listener.accept().await?;
            let request_count = Arc::clone(&request_count);

            tokio::spawn(async move {
                let mut buf = [0; 1024];
                let _ = stream.read(&mut buf).await;

                let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK";
                let _ = stream.write_all(response.as_bytes()).await;

                let mut count = request_count.lock().await;
                *count += 1;
            });
        }
    }

    async fn create_coordinator(
    ) -> anyhow::Result<(CoordinatorClient, UnboundedReceiver<(String, InfoMessage)>)> {
        let (updates_sender, updates_receiver) = unbounded_channel();
        let url = std::env::var("CROWS_COORDINATOR_URL").unwrap_or("127.0.0.1:8282".to_string());
        let coordinator =
            connect_to_coordinator(url, |_| async { Ok(crows::ClientService { updates_sender }) }).await?;
        Ok((coordinator, updates_receiver))
    }

    async fn start_coordinator() -> Result<SocketAddr> {
        let coordinator_addr = "127.0.0.1:8181";
        tokio::spawn(async move {
            crows_coordinator::start_server(8181, 8282).await;
        });
        tokio::time::sleep(Duration::from_secs(1)).await; // Give coordinator time to start
        Ok(coordinator_addr.parse()?)
    }

    async fn start_worker(coordinator_addr: SocketAddr, worker_name: &str) -> Result<()> {
        let worker_addr = format!("{}", coordinator_addr);
        let worker_name = worker_name.to_string();
        tokio::spawn(async move {
            crows_worker::connect_to_coordinator(worker_addr, worker_name)
                .await
                .unwrap();
            Ok::<(), anyhow::Error>(())
        });
        Ok(())
    }

    #[tokio::test]
    async fn test_distributed_stress_test() -> Result<()> {
        // Start the coordinator
        let coordinator_addr = start_coordinator().await?;

        // Start two workers
        start_worker(coordinator_addr, "worker1").await?;
        start_worker(coordinator_addr, "worker2").await?;
        // TODO: implement a way to wait till workers are fully connected
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Start the HTTP server
        let request_count = Arc::new(Mutex::new(0));
        let server_request_count = Arc::clone(&request_count);
        tokio::spawn(start_http_server(server_request_count));

        // Create coordinator client
        let (mut coordinator, mut updates_receiver) = create_coordinator().await?;

        // Upload scenario
        let scenario_name = "test_scenario";
        let scenario_content =
            include_bytes!("../../rust-example/target/wasm32-wasi/release/wasm_example.wasm");
        coordinator
            .upload_scenario(scenario_name.to_string(), scenario_content.to_vec())
            .await??;

        // Start scenario
        let workers_number = 2;
        let mut env_vars = HashMap::new();
        env_vars.insert("SERVER_URL".into(), "http://127.0.0.1:8080".into());
        let (run_id, worker_names) = coordinator
            .start(scenario_name.to_string(), workers_number, env_vars)
            .await??;

        // Collect updates
        let mut updates = Vec::new();
        while let Some(update) = updates_receiver.recv().await {
            if let (_, InfoMessage::Done) = update {
                break;
            }
            updates.push(update);
        }

        tokio::time::sleep(Duration::from_millis(500)).await; // Give some time for all requests to be counted
        let final_request_count = *request_count.lock().await;
        assert!(
            final_request_count == 20,
            "Expected 20 requests, but got {}",
            final_request_count
        );

        let request_info_count = updates
            .iter()
            .filter(|(_, msg)| matches!(msg, InfoMessage::RequestInfo(_)))
            .count();
        assert!(
            request_info_count == 20,
            "Expected 20 RequestInfo updates, but got {}",
            request_info_count
        );

        Ok(())
    }
}
