#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

use std::collections::HashMap;
use std::pin::Pin;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::ops::Deref;

use futures::prelude::*;
use std::future::Future;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::{
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    time::sleep,
};
use tokio_serde::formats::SymmetricalJson;
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use uuid::Uuid;
use futures::TryStreamExt;

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
    ) -> Option<(
        UnboundedSender<Message>,
        UnboundedReceiver<Message>,
        oneshot::Receiver<()>,
    )> {
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

        Some((serialized_sender, deserialized_receiver, close_receiver))
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

pub struct Client {
    sender: UnboundedSender<Message>,
    internal_sender: UnboundedSender<InternalMessage>,
    close_receiver: Option<oneshot::Receiver<()>>,
}

impl Client {
    pub async fn request<
        T: Serialize + std::fmt::Debug + DeserializeOwned + Send + 'static,
        Y: Serialize + std::fmt::Debug + DeserializeOwned + Send + 'static,
    >(
        &self,
        message: T,
    ) -> anyhow::Result<Y> {
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
        self.internal_sender
            .send(InternalMessage::RegisterListener(register_listener))?;
        self.sender.send(message)?;

        // TODO: rewrite to map
        match rx.await {
            Ok(reply) => Ok(serde_json::from_str(&reply)?),
            Err(e) => Err(e)?,
        }
    }

    pub fn new<T, DummyType>(
        sender: UnboundedSender<Message>,
        mut receiver: UnboundedReceiver<Message>,
        mut service: T,
        close_receiver: Option<oneshot::Receiver<()>>,
    ) -> Self
    where
        T: Service<DummyType> + Send + Sync + 'static,
        <T as Service<DummyType>>::Request: Send,
        <T as Service<DummyType>>::Response: Send,
    {
        let (internal_sender, mut internal_receiver) = unbounded_channel();
        let client = Self {
            sender: sender.clone(),
            internal_sender,
            close_receiver,
        };

        tokio::spawn(async move {
            let mut listeners: HashMap<Uuid, oneshot::Sender<String>> = HashMap::new();
            loop {
                tokio::select! {
                    message = receiver.recv() => {
                        match message {
                            Some(message) => {
                                if let Some(reply_to) = message.reply_to {
                                    let reply = listeners.remove(&reply_to).unwrap();
                                    if reply.send(message.message).is_err() {
                                        break;
                                    }
                                } else {
                                    let deserialized = serde_json::from_str::<<T as Service<DummyType>>::Request>(&message.message).unwrap();
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

    pub fn get_close_receiver(&mut self) -> Option<oneshot::Receiver<()>> {
        self.close_receiver.take()
    }

    pub async fn wait(&mut self) {
        if let Some(receiver) = self.close_receiver.take() {
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

pub trait Service<DummyType>: Send + Sync {
    type Response: Send + Serialize;
    type Request: DeserializeOwned + Send;

    fn handle_request(
        &mut self,
        message: Self::Request,
    ) -> Pin<Box<dyn Future<Output = Self::Response> + Send + '_>>;
}
