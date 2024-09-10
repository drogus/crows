#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    rustls::crypto::ring::default_provider()
        .install_default().unwrap();

    let coordinator_address: String =
        std::env::var("COORDINATOR_ADDRESS").unwrap_or("127.0.0.1:8181".into());
    let hostname: String = std::env::var("WORKER_NAME").unwrap();

    crows_worker::connect_to_coordinator(coordinator_address, hostname).await
}
