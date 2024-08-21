#[tokio::main]
pub async fn main() {
    let worker_port: usize = std::env::var("WORKER_PORT")
        .unwrap_or("8181".into())
        .parse()
        .unwrap();
    let client_port: usize = std::env::var("CLIENT_PORT")
        .unwrap_or("8282".into())
        .parse()
        .unwrap();


    crows_coordinator::start_server(worker_port, client_port).await;
}
