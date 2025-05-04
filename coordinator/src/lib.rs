use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crows_utils::services::{
    create_coordinator_server, create_worker_to_coordinator_server, ClientClient, CoordinatorError,
    RunId, WorkerClient,
};
use crows_utils::services::{Coordinator, WorkerToCoordinator};
use crows_utils::InfoMessage;
use crows_wasm::fetch_config;
use futures::future::join_all;
use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;
use uuid::Uuid;

#[derive(Default, Clone)]
struct WorkerToCoordinatorService {
    worker_name: String,
    #[allow(dead_code)]
    worker_id: Uuid,
    runs: Runs,
}

#[derive(Clone)]
struct WorkerEntry {
    client: WorkerClient,
    hostname: String,
}

type Runs = Arc<RwLock<HashMap<RunId, ClientClient>>>;

impl WorkerToCoordinator for WorkerToCoordinatorService {
    async fn ping(&self) -> String {
        "OK".into()
    }

    async fn update(&self, id: RunId, update: InfoMessage) {
        // TODO: we should remove a run if the InfoMessage is saying it's either done
        //       or errored out
        let runs = self.runs.read().await;
        if let Some(client) = runs.get(&id) {
            if let Err(_) = client
                .update(id.clone(), self.worker_name.clone(), update)
                .await
            {
                drop(runs);
                self.runs.write().await.remove(&id);
            }
        }
    }
}

#[derive(Clone)]
struct CoordinatorService {
    scenarios: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    workers: Arc<Mutex<HashMap<Uuid, WorkerEntry>>>,
    runs: Runs,
    // TODO: names like ClientClient suck, maybe it would be nice to use associated
    // types on a trait for this?
    client: ClientClient,
}

impl Coordinator for CoordinatorService {
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
            let mut futures = Vec::new();
            futures.push(async {
                // TODO: handle Result
                if let Err(e) = worker_entry
                    .client
                    .upload_scenario(name.clone(), content.clone())
                    .await
                {
                    // TODO: should we send it to a client
                    eprintln!("Error while uploading scenario to a worker: {e:?}");
                }
            });

            join_all(futures).await;
        }
        self.scenarios.lock().await.insert(name, content);

        Ok(())
    }

    async fn start(
        &self,
        name: String,
        workers_number: usize,
        env_vars: HashMap<String, String>,
    ) -> Result<(RunId, Vec<String>), CoordinatorError> {
        let id = RunId::new();
        // TODO: also this way we will always choose the same workers. in the future we should
        // either always split between all workers or do some kind of round robin
        // TODO: at the moment we split evenly. in the future we could get some kind of diagnostic
        // data from workers in order to determine how much traffic can we push to each worker
        // TODO: creating a runtime is probably fast enough, but I'd like to measure and see
        // if it's not better to keep one around so we don't create it before each test run
        let scenarios = self.scenarios.lock().await;
        let scenario = scenarios
            .get(&name)
            .ok_or(CoordinatorError::NoSuchModule(name.clone()))?
            .to_owned();
        drop(scenarios);

        let env_vars_clone = env_vars.clone();
        let (runtime, _) = tokio::task::spawn_blocking(move || {
            crows_wasm::Runtime::new(&scenario, env_vars_clone)
                .map_err(|err| CoordinatorError::FailedToCreateRuntime(err.to_string()))
        })
        .await
        .map_err(|e| CoordinatorError::FailedToCreateRuntime(e.to_string()))??;

        let (instance, _, mut store) = runtime
            .new_instance()
            .await
            .map_err(|_| CoordinatorError::FailedToCompileModule)?;
        let config = fetch_config(instance, &mut store)
            .await
            .map_err(|err| CoordinatorError::CouldNotFetchConfig(err.to_string()))?
            .split(workers_number);

        let mut runs = self.runs.write().await;
        runs.insert(id.clone(), self.client.clone());
        drop(runs);

        let mut worker_names = Vec::new();
        let workers = self.workers.lock().await;
        if workers_number > workers.len() {
            return Err(CoordinatorError::NotEnoughWorkers(workers.len()));
        }

        for (_, worker_entry) in workers.iter().take(workers_number) {
            worker_names.push(worker_entry.hostname.clone());
            let name = name.clone();
            let config = config.clone();
            let id = id.clone();
            let client = worker_entry.client.clone();
            let env_vars = env_vars.clone();
            tokio::spawn(async move {
                // TODO: at the moment we split config to split the load between each of the
                // workers, which means that if a worker dies, we will not get a full test
                // It would be ideal if we had a way to j
                // client.start(name.clone(), config.clone()).await;
                if let Err(err) = client.start(name, config, id, env_vars).await {
                    eprintln!("Got an error while trying to execute a scenario: {err:?}");
                }
            });
        }

        Ok((id, worker_names))
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

pub async fn start_server(worker_port: usize, client_port: usize) {
    let original_scenarios: Arc<Mutex<HashMap<String, Vec<u8>>>> = Default::default();
    let original_workers: Arc<Mutex<HashMap<Uuid, WorkerEntry>>> = Default::default();

    let scenarios = original_scenarios.clone();
    let workers = original_workers.clone();
    let runs: Runs = Default::default();

    let runs_clone = runs.clone();
    tokio::spawn(async move {
        let server = create_worker_to_coordinator_server(format!("0.0.0.0:{worker_port}"))
            .await
            .unwrap();

        loop {
            let runs_clone = runs_clone.clone();
            let scenarios = scenarios.clone();
            let workers = workers.clone();
            let create_service = |client: WorkerClient| async move {
                let data = client.get_data().await?;

                Ok(WorkerToCoordinatorService {
                    worker_name: data.hostname,
                    worker_id: data.id,
                    runs: runs_clone.clone(),
                })
            };
            if let Ok(client) = server.accept(create_service.clone()).await {
                tokio::spawn(async move {
                    println!("Worker connected");
                    let close_receiver = client.get_close_receiver().await;

                    // sent all the current scenarios to a new worker node
                    let locked = scenarios.lock().await;
                    for (id, content) in locked.iter() {
                        let _ = client.upload_scenario(id.clone(), content.clone()).await;
                    }
                    drop(locked);

                    let mut id = None;
                    if let Ok(data) = client.get_data().await {
                        id = Some(data.id.clone());
                        let mut locked = workers.lock().await;
                        locked.entry(data.id).or_insert(WorkerEntry {
                            client: client.clone(),
                            hostname: data.hostname,
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
        let create_service = |client| async move {
            Ok(CoordinatorService {
                scenarios,
                workers,
                runs: runs.clone(),
                client,
            })
        };
        while let Ok(client) = server.accept(create_service.clone()).await {
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
