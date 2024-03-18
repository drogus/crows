use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crows_wasm::fetch_config;
use futures::future::join_all;
use tokio::sync::Mutex;
use tokio::time::sleep;
use crows_utils::services::{
    create_coordinator_server, create_worker_to_coordinator_server, CoordinatorError, WorkerClient,
    WorkerStatus,
};
use crows_utils::services::{Coordinator, WorkerToCoordinator};
use crows_utils::ModuleId;
use uuid::Uuid;

// TODO: I don't like the fact that we have to wrap the client in Mutex and option. It should
// be easier to match the client object with the request to the service. I should probably
// add a context object at some point.
// TODO: Client should probably be thread safe for easier handling
#[derive(Default)]
struct WorkerToCoordinatorService {
    scenarios: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    workers: Arc<Mutex<HashMap<Uuid, WorkerEntry>>>,
    client: Arc<Mutex<Option<WorkerClient>>>,
}

struct WorkerEntry {
    client: Arc<Mutex<Option<WorkerClient>>>,
    hostname: String,
    status: WorkerStatus,
}

impl WorkerToCoordinator for WorkerToCoordinatorService {
    async fn ping(&mut self) -> String {
        "OK".into()
    }
}

#[derive(Clone, Default)]
struct CoordinatorService {
    scenarios: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    workers: Arc<Mutex<HashMap<Uuid, WorkerEntry>>>,
}

impl Coordinator for CoordinatorService {
    async fn update_status(&self, status: WorkerStatus, id: Uuid) {
        todo!()
        // let workers = self.workers.lock().await;
        // let mut worker = workers.get(&id).unwrap();
        // worker.status = status;
    }

    async fn upload_scenario(
        &self,
        name: String,
        content: Vec<u8>,
    ) -> Result<(), CoordinatorError> {
        // TODO: to send bandwidth maybe it will be worth it to gzip the data? we would be 
        // gzipping once and sending to N clients
        //
        // send each uploaded scenario to all of the workers
        for (_, worker_entry) in self.workers.lock().await.iter() {
            let locked = worker_entry.client.lock();
            let mut futures = Vec::new();
            futures.push(async {
                if let Some(client) = locked.await.as_mut() {
                    // TODO: handle Result
                    client.upload_scenario(name.clone(), content.clone()).await;
                }
            });

            join_all(futures).await;
        }
        self.scenarios.lock().await.insert(name, content);

        Ok(())
    }

    async fn start(&self, name: String, workers_number: usize) -> Result<(), CoordinatorError> {
        // TODO: we should check if we have enough workers
        // TODO: also this way we will always choose the same workers. in the future we should
        // either always split between all workers or do some kind of round robin
        // TODO: at the moment we split evenly. in the future we could get some kind of diagnostic
        // data from workers in order to determine how much traffic can we push to each worker
        // TODO: creating a runtime is probably fast enough, but I'd like to measure and see
        // if it's not better to keep one around so we don't create it before each test run
        let runtime = crows_wasm::Runtime::new().map_err(|err| CoordinatorError::FailedToCreateRuntime(err.to_string()))?;
        let mut instance = {
            let scenarios = self.scenarios.lock().await;
            let scenario = scenarios.get(&name).ok_or(CoordinatorError::NoSuchModule(name.clone()))?;
            let (instance, _) = runtime.compile_instance(&scenario).await.map_err(|_| CoordinatorError::FailedToCompileModule)?;
            instance
        };
        let config = fetch_config(&mut instance).await.map_err(|err| CoordinatorError::CouldNotFetchConfig(err.to_string()))?.split(workers_number);

        for (_, worker_entry) in self.workers.lock().await.iter().take(workers_number) {
            if let Some(client) = worker_entry.client.lock().await.as_mut() {
                // TODO: at the moment we split config to split the load between each of the
                // workers, which means that if a worker dies, we will not get a full test
                // It would be ideal if we had a way to j
                client.start(name.clone(), config.clone()).await;
            }
        }

        Ok(())
    }

    async fn list_workers(&self) -> Vec<String> {
        self.workers
            .lock()
            .await
            .iter()
            .map(|(_, data)| data.hostname.clone())
            .collect()
    }
}

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

    let original_scenarios: Arc<Mutex<HashMap<String, Vec<u8>>>> = Default::default();
    let original_workers: Arc<Mutex<HashMap<Uuid, WorkerEntry>>> = Default::default();

    let scenarios = original_scenarios.clone();
    let workers = original_workers.clone();
    tokio::spawn(async move {
        let server = create_worker_to_coordinator_server(format!("0.0.0.0:{worker_port}"))
            .await
            .unwrap();

        loop {
            let wrapped_client: Arc<Mutex<Option<WorkerClient>>> = Default::default();
            let service = WorkerToCoordinatorService {
                scenarios: scenarios.clone(),
                client: wrapped_client.clone(),
                workers: workers.clone(),
            };

            let scenarios = scenarios.clone();
            let workers = workers.clone();
            if let Some(mut client) = server.accept(service).await {
                tokio::spawn(async move {
                    println!("Worker connected");
                    let close_receiver = client.get_close_receiver();

                    // sent all the current scenarios to a new worker node
                    let locked = scenarios.lock().await;
                    for (id, content) in locked.iter() {
                        let _ = client.upload_scenario(id.clone(), content.clone()).await;
                    }
                    drop(locked);

                    let mut locked = wrapped_client.lock().await;
                    *locked = Some(client);
                    drop(locked);

                    let mut id = None;
                    if let Ok(data) = wrapped_client
                        .lock()
                        .await
                        .as_mut()
                        .unwrap()
                        .get_data()
                        .await
                    {
                        id = Some(data.id.clone());
                        let mut locked = workers.lock().await;
                        locked.entry(data.id).or_insert(WorkerEntry {
                            client: wrapped_client.clone(),
                            hostname: data.hostname,
                            status: WorkerStatus::Busy
                        });
                        drop(locked);
                    }

                    if let Some(r) = close_receiver {
                        let _ = r.await;
                    }

                    // Worker is closed, let's remove it from the list
                    if let Some(id) = id {
                        workers.lock().await.remove(&id);
                    }
                });
            } else {
                println!("Closing");
                break;
            }
        }
    });

    let scenarios = original_scenarios.clone();
    let workers = original_workers.clone();
    tokio::spawn(async move {
        let server = create_coordinator_server(format!("0.0.0.0:{client_port}"))
            .await
            .unwrap();
        let service = CoordinatorService { scenarios, workers };

        while let Some(mut client) = server.accept(service.clone()).await {
            tokio::spawn(async move {
                // we don't send any messages for the client, so in order to not drop it
                // (and thus disconnect), we need to wait
                client.wait().await;
            });
        }
    });

    // there should be a nicer way to wait for all of the servers to shut down, but for now
    // this is the simplest way
    loop {
        sleep(Duration::from_secs(1)).await;
    }
}
