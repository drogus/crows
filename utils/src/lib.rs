use std::collections::HashMap;
use std::io::ErrorKind;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use futures::prelude::*;
use futures::TryStreamExt;
use services::{RunInfo, RequestInfo, IterationInfo};
use std::future::Future;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::sync::RwLock;
use tokio::sync::{
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    oneshot,
};
use tokio_serde::formats::SymmetricalJson;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use uuid::Uuid;

pub use serde;
pub use tokio;
pub mod services;
pub use crows_service;

pub struct Server {
    listener: TcpListener,
}

impl Server {
    pub async fn accept(
        &self,
    ) -> Result<(
        UnboundedSender<Message>,
        UnboundedReceiver<Message>,
        oneshot::Receiver<()>,
    ), std::io::Error> {
        let (socket, _) = self.listener.accept().await?;
        let (reader, writer) = socket.into_split();

        // Delimit frames using a length header
        let length_delimited = FramedRead::new(reader, LengthDelimitedCodec::new());

        // Deserialize frames
        let mut deserialized =
            tokio_serde::SymmetricallyFramed::new(length_delimited, SymmetricalJson::default());

        let length_delimited = FramedWrite::new(writer, LengthDelimitedCodec::new());

        let mut serialized = tokio_serde::SymmetricallyFramed::new(
            length_delimited,
            SymmetricalJson::<Message>::default(),
        );

        let (serialized_sender, mut serialized_receiver) = unbounded_channel::<Message>();
        let (deserialized_sender, deserialized_receiver) = unbounded_channel::<Message>();
        let (close_sender, close_receiver) = oneshot::channel::<()>();

        tokio::spawn(async move {
            while let Some(message) = serialized_receiver.recv().await {
                if let Err(err) = serialized.send(message).await {
                    println!("Error while sending message: {err:?}");
                    break;
                }
            }
        });

        tokio::spawn(async move {
            // TODO: handle Err
            while let Ok(Some(message)) = deserialized.try_next().await {
                if let Err(err) = deserialized_sender.send(message) {
                    println!("Error while sending message: {err:?}");
                    break;
                }
            }

            if let Err(e) = close_sender.send(()) {
                println!("Got an error when sending to a close_sender: {e:?}");
            }
        });

        Ok((serialized_sender, deserialized_receiver, close_receiver))
    }
}

pub async fn create_server<A>(addr: A) -> Result<Server, std::io::Error>
where
    A: ToSocketAddrs,
{
    // Bind a server socket
    let listener = TcpListener::bind(addr).await?;

    // println!("listening on {:?}", listener.local_addr());

    Ok(Server { listener })
}

pub async fn create_client<A>(
    addr: A,
) -> Result<(UnboundedSender<Message>, UnboundedReceiver<Message>), std::io::Error>
where
    A: ToSocketAddrs,
{
    // Bind a server socket
    let socket = TcpStream::connect(addr).await?;

    let (reader, writer) = socket.into_split();

    // Delimit frames using a length header
    let length_delimited = FramedWrite::new(writer, LengthDelimitedCodec::new());

    // Serialize frames with JSON
    let mut serialized =
        tokio_serde::SymmetricallyFramed::new(length_delimited, SymmetricalJson::default());

    // Delimit frames using a length header
    let length_delimited = FramedRead::new(reader, LengthDelimitedCodec::new());

    // Deserialize frames
    let mut deserialized =
        tokio_serde::SymmetricallyFramed::new(length_delimited, SymmetricalJson::default());

    let (serialized_sender, mut serialized_receiver) = unbounded_channel::<Message>();
    let (deserialized_sender, deserialized_receiver) = unbounded_channel::<Message>();

    tokio::spawn(async move {
        while let Some(message) = serialized_receiver.recv().await {
            if let Err(err) = serialized.send(message).await {
                println!("Error while sending message: {err:?}");
                break;
            }
        }
    });

    tokio::spawn(async move {
        // TODO: handle Err
        while let Ok(Some(message)) = deserialized.try_next().await {
            if let Err(err) = deserialized_sender.send(message) {
                println!("Error while sending message: {err:?}");
                break;
            }
        }
    });

    Ok((serialized_sender, deserialized_receiver))
}

#[derive(Debug)]
struct RegisterListener {
    respond_to: oneshot::Sender<String>,
    message_id: Uuid,
}

#[derive(Debug)]
enum InternalMessage {
    RegisterListener(RegisterListener),
}

#[derive(Clone)]
pub struct Client {
    inner: Arc<RwLock<ClientInner>>,
    sender: UnboundedSender<Message>,
    internal_sender: UnboundedSender<InternalMessage>,
}

struct ClientInner {
    close_receiver: Option<oneshot::Receiver<()>>,
}

impl Client {
    pub async fn request<
        T: Serialize + std::fmt::Debug + DeserializeOwned + Send + 'static,
        Y: Serialize + std::fmt::Debug + DeserializeOwned + Send + 'static,
    >(
        &self,
        message: T,
    ) -> Result<Y, std::io::Error> {
        let message = Message {
            id: Uuid::new_v4(),
            reply_to: None,
            message: serde_json::to_string(&message)?,
            message_type: std::any::type_name::<T>().to_string(),
        };

        let (tx, rx) = oneshot::channel::<String>();
        let register_listener = RegisterListener {
            respond_to: tx,
            message_id: message.id,
        };
        self.send_internal(InternalMessage::RegisterListener(register_listener))
            .await?;
        self.send(message).await?;

        let reply = rx.await.expect("Listener channel closed without sending a response");
        Ok(serde_json::from_str(&reply)?)
    }

    async fn send(&self, message: Message) -> Result<(), std::io::Error> {
        Ok(self.sender.send(message).map_err(|_| std::io::Error::new(ErrorKind::ConnectionAborted, "connection closed"))?)
    }

    async fn send_internal(&self, message: InternalMessage) -> Result<(), std::io::Error> {
        Ok(self.internal_sender.send(message).map_err(|_| std::io::Error::new(ErrorKind::ConnectionAborted, "connection closed"))?)
    }

    pub async fn new<T, DummyType, F, Fut>(
        sender: UnboundedSender<Message>,
        mut receiver: UnboundedReceiver<Message>,
        close_receiver: Option<oneshot::Receiver<()>>,
        create_service_callback: F,
    ) -> Result<<T as Service<DummyType>>::Client, std::io::Error>
    where
        T: Service<DummyType> + Send + Sync + 'static + Clone,
        <T as Service<DummyType>>::Request: Send,
        <T as Service<DummyType>>::Response: Send,
        <T as Service<DummyType>>::Client: ClientTrait + Clone + Send + Sync + 'static,
        F: FnOnce(<T as Service<DummyType>>::Client) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, std::io::Error>> + Send
    {
        let (internal_sender, mut internal_receiver) = unbounded_channel();
        let client = T::Client::new(Self {
            inner: Arc::new(RwLock::new(ClientInner { close_receiver })),
            sender: sender.clone(),
            internal_sender,
        });

        let service = create_service_callback(client.clone()).await?;
        let client_clone = client.clone();
        tokio::spawn(async move {
            let mut listeners: HashMap<Uuid, oneshot::Sender<String>> = HashMap::new();
            loop {
                tokio::select! {
                    message = receiver.recv() => {
                        match message {
                            Some(message) => {
                                if let Some(reply_to) = message.reply_to {
                                    // TODO: Should this expect be changed into a log line?
                                    let reply = listeners.remove(&reply_to).expect("Listener should be registered for a message");
                                    if reply.send(message.message).is_err() {
                                        break;
                                    }
                                } else {
                                    let service_clone = service.clone();
                                    let sender_clone = sender.clone();
                                    tokio::spawn(async move {
                                        let deserialized = serde_json::from_str::<<T as Service<DummyType>>::Request>(&message.message).unwrap();
                                        let response = service_clone.handle_request(deserialized).await;

                                        let message = Message {
                                            id: Uuid::new_v4(),
                                            reply_to: Some(message.id),
                                            message: serde_json::to_string(&response).unwrap(),
                                            message_type: std::any::type_name::<T>().to_string(),
                                        };
                                        sender_clone.send(message).unwrap();
                                    });
                                }
                            },
                            None => break,
                        }
                    }
                    internal_message = internal_receiver.recv() => {
                        match internal_message {
                            Some(internal_message) => {
                                match internal_message {
                                    InternalMessage::RegisterListener(register_listener) => {
                                        listeners.insert(register_listener.message_id, register_listener.respond_to);
                                    }
                                }
                            },
                            None => break
                        }
                    }
                }
            }
        });

        Ok(client)
    }

    pub async fn get_close_receiver(&self) -> Option<oneshot::Receiver<()>> {
        let mut inner = self.inner.write().await;
        inner.close_receiver.take()
    }

    pub async fn wait(&self) {
        let mut inner = self.inner.write().await;
        if let Some(receiver) = inner.close_receiver.take() {
            if let Err(e) = receiver.await {
                println!("Got an error when waiting for oneshot receiver: {e:?}");
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub id: Uuid,
    pub reply_to: Option<Uuid>,
    pub message: String,
    pub message_type: String,
}

pub trait ClientTrait {
    fn new(client: Client) -> Self;
}

/// The DummyType here is needed, because in the `service` macro we implement
/// service on a generic type. This, in turn, is needed because I wanted to
/// allow for a service definition after specifying the impl.
/// For example we define a Worker RPC service in a shared crate/file. In there
/// we want to only define the interface, but in order for the service to work properly
/// the Service trait has to be also implemented. It's best to do it in the macro
/// itself, cause it requires a lot of boilerplate, but when the macro runs, we don't
/// have the actual service defined yet.
///
/// So if we could define all of it in one file it would be something like:
///
///     trait Worker {
///         async fn ping(&self) -> String;
///     }
///
///     struct WorkerService {}
///
///     impl Worker for WorkerService {
///         async fn ping(&self) -> String { todo!() }
///     }
///
///     impl Service for WorkrService {
///         type Request = WorkerRequest;
///         type Response = WorkerResponse;
///
///         fn handle_request(...) { .... }
///     }
///
/// The problem is, we don't want to require implementation of the service to live
/// in the same place where the definition lives. That's why it's better to only
/// implement Service for a generic type and thus allow for it to be applied
/// only when the type is actually created, for example:
///
///     impl<T> Service for T
///     where T: Worker + Send + Sync { }
///
/// The issue here is that this results in a "conflicting implementation" error if
/// there is more than one `impl` of this type present. The reason is future proofing.
/// For example consider the previous impl and another one for another service
///
///     impl<T> Service for T
///     where T: Coordinator + Send + Sync { }
///
/// While we know that we don't want to implement both `Coordinator` and `Worker`
/// traits on the same type, Rust doesn't. The solution is to add a "dummy type"
/// to the service implementation and thus narrow down the impl to a specific generic
/// type, for example:
///
///     struct DummyWorkerService {}
///
///     impl<T> Service<DummyWorkerService> for T
///     where T: Worker + Send + Sync { }
///
/// Now the impl is only considered for a specific Service type and the only
/// additional requirement is that now we have to include the dummy type when
/// specifycing the service, for example if we accept the Worker service as an
/// argument we say:
///
///     fn foo<T>(service: T)
///         where T: Service<DummyWorkerService> { }
///
pub trait Service<DummyType>: Send + Sync {
    type Response: Send + Serialize;
    type Request: DeserializeOwned + Send;
    type Client: ClientTrait + Clone + Send + Sync;

    fn handle_request(
        &self,
        message: Self::Request,
    ) -> Pin<Box<dyn Future<Output = Self::Response> + Send + '_>>;
}

pub async fn process_info_handle(handle: &mut InfoHandle) -> RunInfo {
    // TODO: I don't like the way the info here is handled. I would much rather 
    // make it so there's a way to send a message from a worker that will be passed to
    // to the client through coordinator
    let mut run_info: RunInfo = Default::default();
    run_info.done = false;

    while let Ok(update) = handle.receiver.try_recv() {
        match update {
            InfoMessage::Stderr(buf) => run_info.stderr.push(buf),
            InfoMessage::Stdout(buf) => run_info.stdout.push(buf),
            InfoMessage::RequestInfo(info) => run_info.request_stats.push(info),
            InfoMessage::IterationInfo(info) => run_info.iteration_stats.push(info),
            InfoMessage::InstanceCheckedOut => run_info.active_instances_delta += 1,
            InfoMessage::InstanceReserved => run_info.capacity_delta += 1,
            InfoMessage::InstanceCheckedIn => run_info.active_instances_delta -= 1,
            InfoMessage::TimingUpdate((elapsed, left)) => {
                run_info.elapsed = Some(elapsed);
                run_info.left = Some(left);
            }
            InfoMessage::Done => run_info.done = true,
            InfoMessage::PrepareError(message) => {

            }
            InfoMessage::RunError(message) => {

            }
        }
    }

    run_info
}

// TODO: I don't like that name, I think it should be changed
pub enum InfoMessage {
    Stderr(Vec<u8>),
    Stdout(Vec<u8>),
    RequestInfo(RequestInfo),
    IterationInfo(IterationInfo),
    // TODO: I'm not sure if shoving any kind of update here is a good idea,
    // but at the moment it's the easiest way to pass data back to the client,
    // so I'm going with it. I'd like to revisit it in the future, though and
    // consider alternatives
    InstanceCheckedOut,
    InstanceReserved,
    InstanceCheckedIn,
    // elapsed, left
    TimingUpdate((Duration, Duration)),
    Done,
    PrepareError(String),
    RunError(String),
}

pub struct InfoHandle {
    pub receiver: UnboundedReceiver<InfoMessage>,
}
