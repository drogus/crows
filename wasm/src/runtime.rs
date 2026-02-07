use crate::{HTTPMethod, HTTPRequest};
use anyhow::Result;
use crows_utils::services::RequestInfo;
use local::crows::types::HttpMethod;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{mpsc::UnboundedSender, oneshot, RwLock};
use tokio::time::Instant;
use wasmtime::component::{bindgen, Component};

use crate::http_client::Client;
use crate::{Environment, Instance, WasiHostCtx};
use crows_utils::{InfoHandle, InfoMessage};

pub enum RuntimeMessage {
    RunTest(oneshot::Sender<()>),
}

#[derive(Clone)]
pub struct InstanceHandle {
    pub inner: Option<InstanceHandleInner>,
}

#[derive(Clone)]
pub struct InstanceHandleInner {
    pub sender: UnboundedSender<RuntimeMessage>,
    pub runtime: Arc<RwLock<RuntimeInner>>,
}

impl InstanceHandle {
    pub async fn run_test(&self) -> Result<()> {
        let (sender, receiver) = oneshot::channel();
        let inner = self
            .inner
            .iter()
            .next()
            .expect("Inner should be available before drop");
        let instant = Instant::now();
        inner.sender.send(RuntimeMessage::RunTest(sender))?;
        receiver.await?;
        let latency = instant.elapsed();
        inner
            .runtime
            .write()
            .await
            .info_sender
            .send(InfoMessage::IterationInfo(
                crows_utils::services::IterationInfo { latency },
            ))?;
        Ok(())
    }
}

pub struct RuntimeInner {
    pub instances: VecDeque<InstanceHandle>,
    pub info_sender: UnboundedSender<InfoMessage>,
    pub length: usize,
}

pub struct Runtime {
    pub environment: Environment,
    pub component_pre: CrowsPre<WasiHostCtx>,
    pub inner: Arc<RwLock<RuntimeInner>>,
    pub info_sender: UnboundedSender<InfoMessage>,
    pub env_vars: HashMap<String, String>,
}

impl Drop for InstanceHandle {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            tokio::spawn(async move {
                let mut runtime = inner.runtime.write().await;
                runtime
                    .checkin_instance(InstanceHandle {
                        inner: Some(inner.clone()),
                    })
                    .await;
            });
        }
    }
}

bindgen!({
    world: "crows",
    path: "crows.wit",
    imports: { default: async | trappable },
    exports: { default: async },
});

pub struct HostComponent {
    client: Client,
    request_info_sender: UnboundedSender<RequestInfo>,
}

impl HostComponent {
    pub fn new(client: Client, request_info_sender: UnboundedSender<RequestInfo>) -> Self {
        Self {
            client,
            request_info_sender,
        }
    }

    pub fn clear_connections(&mut self) {
        self.client.clear_connections();
    }
}

impl host::Host for HostComponent {
    async fn http_request(
        &mut self,
        request: host::Request,
    ) -> wasmtime::Result<std::result::Result<host::Response, host::HttpError>> {
        let method = match request.method {
            HttpMethod::Get => HTTPMethod::GET,
            HttpMethod::Post => HTTPMethod::POST,
            HttpMethod::Put => HTTPMethod::PUT,
            HttpMethod::Delete => HTTPMethod::DELETE,
            HttpMethod::Patch => HTTPMethod::PATCH,
            HttpMethod::Head => HTTPMethod::HEAD,
            HttpMethod::Options => HTTPMethod::OPTIONS,
            HttpMethod::Trace => HTTPMethod::TRACE,
        };

        let request = HTTPRequest {
            url: request.uri,
            method,
            headers: request.headers.into_iter().map(|(k, v)| (k, v)).collect(),
            body: request.body,
        };
        let (http_response, request_info) = match self
            .client
            .http_request(request)
            .await
        {
            Ok(resp) => resp,
            Err(e) => return Ok(Err(host::HttpError { message: e.message })),
        };

        let _ = self.request_info_sender.send(request_info);

        let response = host::Response {
            status: http_response.status,
            headers: http_response.headers.into_iter().collect(),
            body: http_response.body,
        };

        Ok(Ok(response))
    }
}

impl Runtime {
    pub fn send_update(&self, update: InfoMessage) -> Result<()> {
        Ok(self.info_sender.send(update)?)
    }

    pub async fn capacity(&self) -> usize {
        self.inner.read().await.length
    }

    pub async fn active_count(&self) -> usize {
        let inner = self.inner.read().await;
        inner.length - inner.instances.len()
    }

    pub fn new(content: &Vec<u8>, env_vars: HashMap<String, String>) -> Result<(Self, InfoHandle)> {
        let environment = Environment::new()?;
        let component = Component::from_binary(&environment.engine, content.as_slice())?;
        let pre = environment.linker.instantiate_pre(&component)?;
        let crows_pre = CrowsPre::new(pre)?;

        let (info_sender, info_receiver) = tokio::sync::mpsc::unbounded_channel();

        let info_handle = InfoHandle {
            receiver: info_receiver,
        };

        Ok((
            Self {
                component_pre: crows_pre,
                environment,
                inner: Arc::new(RwLock::new(RuntimeInner {
                    instances: VecDeque::new(),
                    info_sender: info_sender.clone(),
                    length: 0,
                })),
                info_sender,
                env_vars,
            },
            info_handle,
        ))
    }

    pub async fn new_instance(
        &self,
    ) -> Result<(Instance, InfoHandle, wasmtime::Store<WasiHostCtx>)> {
        Instance::new(&self.environment, &self.component_pre, &self.env_vars).await
    }

    pub async fn reserve_instance(&mut self) -> Result<()> {
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let inner = InstanceHandleInner {
            sender,
            runtime: self.inner.clone(),
        };
        let handle = InstanceHandle { inner: Some(inner) };

        let (instance, mut info_handle, store) =
            Instance::new(&self.environment, &self.component_pre, &self.env_vars).await?;

        let info_sender = self.info_sender.clone();
        tokio::spawn(async move {
            while let Some(message) = info_handle.receiver.recv().await {
                if let Err(_) = info_sender.send(message) {
                    break;
                }
            }
        });

        tokio::spawn(async move {
            let mut store = store;
            let mut receiver = receiver;
            let mut instance = instance;

            while let Some(message) = receiver.recv().await {
                match message {
                    RuntimeMessage::RunTest(sender) => {
                        if let Err(err) = crate::run_wasm(&mut instance, &mut store).await {
                            eprintln!("Error running WASM: {:?}", err);
                        }
                        sender.send(()).unwrap();
                    }
                }
            }
        });

        let mut inner = self.inner.write().await;
        inner.instances.push_back(handle);
        inner.length += 1;
        let _ = self.info_sender.send(InfoMessage::InstanceReserved);

        Ok(())
    }

    pub async fn checkout_instance(&self) -> Option<InstanceHandle> {
        let mut inner = self.inner.write().await;
        inner.instances.pop_front()
    }

    pub async fn checkin_instance(&self, instance_handle: InstanceHandle) {
        self.inner
            .write()
            .await
            .checkin_instance(instance_handle)
            .await;
    }

    pub async fn checkout_or_create_instance(&mut self) -> Result<InstanceHandle> {
        loop {
            if let Some(handle) = self.checkout_instance().await {
                let _ = self.info_sender.send(InfoMessage::InstanceCheckedOut);
                return Ok(handle);
            } else {
                self.reserve_instance().await?;
            }
        }
    }
}

impl RuntimeInner {
    pub async fn checkin_instance(&mut self, instance_handle: InstanceHandle) {
        self.instances.push_back(instance_handle);
        let _ = self.info_sender.send(InfoMessage::InstanceCheckedIn);
    }
}
