use std::{collections::HashMap, env::args_os, time::Duration};

use lunatic::{
    serializer::Bincode, sleep, spawn, Mailbox, MessageSignal, Process, ProcessDiedSignal,
    WasmModule,
};
use serde::{Deserialize, Serialize};

use uuid::Uuid;

use utils::services::{connect_to_worker_to_coordinator, Worker, WorkerData, WorkerError};

#[derive(Serialize, Deserialize, Clone)]
struct WorkerService {
    scenarios: HashMap<String, Vec<u8>>,
    currently_running: Option<Process<()>>,
    hostname: String,
}

impl Worker for WorkerService {
    fn upload_scenario(&mut self, name: String, content: Vec<u8>) {
        self.scenarios.insert(name, content);
    }

    fn ping(&self) -> String {
        todo!()
    }

    fn start(&self, name: String, concurrency: usize) -> Result<(), WorkerError> {
        let scenario = self
            .scenarios
            .get(&name)
            .ok_or(WorkerError::ScenarioNotFound)?
            .clone();

        let args = (scenario, name, concurrency);
        let _process = spawn!(|args, mailbox: Mailbox<()>| {
            let (scenario, name, concurrency) = args;

            println!("Running {name} scenario with {concurrency} concurrency.");

            let module = WasmModule::new(&scenario).unwrap();
            let monitorable = mailbox.monitorable();
            let mut processes = Vec::new();
            for _ in 0..concurrency {
                match module.spawn::<(), Bincode>("_start", &[]) {
                    Ok(process) => {
                        processes.push(process.id());
                        monitorable.monitor(process);
                    }
                    Err(e) => {
                        println!("Could not start process {name}: {e:?}");
                    }
                }
            }

            loop {
                match monitorable.receive() {
                    MessageSignal::Signal(ProcessDiedSignal(id)) => {
                        if let Some(index) = processes.iter().position(|x| *x == id) {
                            processes.remove(index);
                            if processes.is_empty() {
                                break;
                            }
                        }
                    }
                    MessageSignal::Message(_) => {}
                }
            }
        });

        Ok(())
    }

    fn get_data(&self) -> WorkerData {
        WorkerData {
            id: Uuid::new_v4(),
            hostname: self.hostname.clone(),
        }
    }
}

#[lunatic::main]
fn main(mailbox: Mailbox<()>) {
    let mut args = args_os().skip(1);
    let coordinator_address = args.next().unwrap().to_str().unwrap().to_string();
    let hostname = args.next().unwrap().to_str().unwrap().to_string();

    println!("Starting with hostname: {hostname}");
    loop {
        let mailbox = mailbox.monitorable();

        let hostname = hostname.clone();
        let coordinator_address = coordinator_address.clone();
        let args = (coordinator_address, hostname);
        let main_process = spawn!(|args, mailbox: Mailbox<String>| {
            let (coordinator_address, hostname) = args;
            let scenarios: HashMap<String, Vec<u8>> = Default::default();
            let service = WorkerService {
                scenarios,
                currently_running: None,
                hostname,
            };

            println!("Connecting to {coordinator_address}");
            let mut client =
                connect_to_worker_to_coordinator(coordinator_address, service, mailbox).unwrap();

            loop {
                client.ping().unwrap();
                sleep(Duration::from_secs(1));
            }
        });
        mailbox.monitor(main_process);

        loop {
            if let MessageSignal::Signal(ProcessDiedSignal(id)) = mailbox.receive() {
                println!("Process {id} died, reconnecing in 5s");
                sleep(Duration::from_secs(5));
                break;
            }
        }
    }
}
