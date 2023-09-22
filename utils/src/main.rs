#![feature(async_fn_in_trait)]

use std::collections::HashMap;
use std::marker::PhantomData;
use std::pin::Pin;

use futures::prelude::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration};
use tokio_serde::formats::*;
use tokio_util::codec::FramedRead;
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};
use uuid::Uuid;

use service::service;

#[service(variant = "server", other_side = Worker)]
trait Coordinator {
    async fn hello(&self, name: String) -> String;
}

impl Coordinator for CoordinatorService {
    async fn hello(&self, name: String) -> String {
        format!("Hello from Coordinator {name}!")
    }
}

#[service(variant = "client", other_side = Coordinator)]
trait Worker {
    async fn hello(&self, name: String) -> String;
}

impl Worker for WorkerService {
    async fn hello(&self, name: String) -> String {
        format!("Hello from Worker {name}!")
    }
}

#[tokio::main]
pub async fn main() {
    tokio::spawn(async {
        let server = create_coordinator_server().await.unwrap();
        while let Some(worker_client) = server.accept().await {
            let response = worker_client.hello("coordinator".into()).await;
            println!("Response from worker: {response:?}");
        }
    });

    sleep(Duration::from_millis(100)).await;
    let world_client = connect_to_coordinator().await.unwrap();

    let response = world_client.hello("worker 1".into()).await;
    println!("Response from coordinator: {response:?}");

    sleep(Duration::from_millis(100000)).await;
}

pub struct Server {
    listener: TcpListener,
}

impl Server {
    pub async fn accept(&self) -> Option<(UnboundedSender<Message>, UnboundedReceiver<Message>)> {
        let (socket, _) = self.listener.accept().await.ok()?;
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

        Some((serialized_sender, deserialized_receiver))
    }
}

pub async fn create_server() -> Result<Server, std::io::Error> {
    // Bind a server socket
    let listener = TcpListener::bind("127.0.0.1:17653").await?;

    // println!("listening on {:?}", listener.local_addr());

    Ok(Server { listener })
}

async fn create_client() -> Result<(UnboundedSender<Message>, UnboundedReceiver<Message>), std::io::Error> {
    // Bind a server socket
    let socket = TcpStream::connect("127.0.0.1:17653").await?;

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
pub struct RegisterListener {
    respond_to: oneshot::Sender<String>,
    message_id: Uuid,
}

#[derive(Debug)]
enum InternalMessage {
    RegisterListener(RegisterListener),
}

struct Client {
    sender: UnboundedSender<Message>,
    internal_sender: UnboundedSender<InternalMessage>,
}

impl Client {
    async fn request<
        T: Serialize + std::fmt::Debug + DeserializeOwned + Send + 'static,
        Y: Serialize + std::fmt::Debug + DeserializeOwned + Send + 'static,
    >(
        &self,
        message: T,
    ) -> Y {
        let message = Message {
            id: Uuid::new_v4(),
            reply_to: None,
            message: serde_json::to_string(&message).unwrap(),
            message_type: std::any::type_name::<T>().to_string(),
        };

        let (tx, rx) = oneshot::channel::<String>();
        let register_listener = RegisterListener {
            respond_to: tx,
            message_id: message.id,
        };
        self.internal_sender
            .send(InternalMessage::RegisterListener(register_listener))
            .unwrap();
        self.sender.send(message).unwrap();

        match rx.await {
            Ok(reply) => serde_json::from_str(&reply).unwrap(),
            Err(e) => panic!("error: {e:?}"),
        }
    }

    fn new<T>(
        sender: UnboundedSender<Message>,
        mut receiver: UnboundedReceiver<Message>,
        service: T,
    ) -> Self
    where
        T: Service + Send + Sync + 'static,
        <T as Service>::Request: Send,
        <T as Service>::Response: Send,
    {
        let (internal_sender, mut internal_receiver) = unbounded_channel();
        let client = Self {
            sender: sender.clone(),
            internal_sender,
        };

        tokio::spawn(async move {
            let mut listeners: HashMap<Uuid, oneshot::Sender<String>> = HashMap::new();
            loop {
                tokio::select! {
                    message = receiver.recv() => {
                        match message {
                            Some(message) => {
                                // println!("Got a message: {:?}", message);
                                if let Some(reply_to) = message.reply_to {
                                    let reply = listeners.remove(&reply_to).unwrap();
                                    reply.send(message.message).unwrap();
                                } else {
                                    let deserialized = serde_json::from_str::<<T as Service>::Request>(&message.message).unwrap();
                                    let response = service.handle_request(deserialized).await;

                                    let message = Message {
                                        id: Uuid::new_v4(),
                                        reply_to: Some(message.id),
                                        message: serde_json::to_string(&response).unwrap(),
                                        message_type: std::any::type_name::<T>().to_string(),
                                    };
                                    sender.send(message).unwrap();
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
                                        // println!("register_listener {:?}", register_listener);
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

        client
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    id: Uuid,
    reply_to: Option<Uuid>,
    message: String,
    message_type: String,
}

pub trait Service: Send + Sync {
    type Response: Send + Serialize;
    type Request: DeserializeOwned + Send;

    fn handle_request(
        &self,
        message: Self::Request,
    ) -> Pin<Box<dyn Future<Output = Self::Response> + Send + '_>>;
}
