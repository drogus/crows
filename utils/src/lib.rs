#![feature(async_fn_in_trait)]
#![feature(return_position_impl_trait_in_trait)]

use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::ops::Deref;

#[cfg(feature = "async")]
use futures::prelude::*;
#[cfg(feature = "async")]
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
#[cfg(feature = "async")]
use tokio::{
    sync::{
        mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
        oneshot,
    },
    time::sleep,
};
#[cfg(feature = "async")]
use tokio_serde::formats::*;
#[cfg(feature = "async")]
use tokio_util::codec::{FramedRead, FramedWrite, LengthDelimitedCodec};
use uuid::Uuid;

pub use serde;
#[cfg(feature = "async")]
pub use tokio;
pub mod services;
pub use service;

#[cfg(feature = "lunatic")]
use std::io::{Read, Write};

#[cfg(feature = "async")]
pub struct Server {
    listener: TcpListener,
}

#[cfg(feature = "async")]
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

#[cfg(feature = "async")]
pub async fn create_server<A>(addr: A) -> Result<Server, std::io::Error>
where
    A: ToSocketAddrs,
{
    // Bind a server socket
    let listener = TcpListener::bind(addr).await?;

    // println!("listening on {:?}", listener.local_addr());

    Ok(Server { listener })
}

#[cfg(feature = "lunatic")]
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

#[cfg(feature = "lunatic")]
pub fn process_write_loop(
    write: &mut lunatic::net::TcpStream,
    mailbox: &Mailbox<Message>,
) -> anyhow::Result<()> {
    let message = mailbox.receive();
    let str = serde_json::to_string(&message)?;
    let bytes = str.as_bytes();
    let len = bytes.len();
    let mut len_bytes = vec![];
    len_bytes.write_u32::<BigEndian>(len as u32)?;

    let mut reply = vec![];
    reply.extend_from_slice(&len_bytes);
    reply.extend_from_slice(bytes);

    write.write(&reply)?;

    Ok(())
}

#[cfg(feature = "lunatic")]
pub fn create_client<A>(
    addr: A,
) -> Result<(Process<Message>, lunatic::net::TcpStream), std::io::Error>
where
    A: lunatic::net::ToSocketAddrs,
{
    let stream = lunatic::net::TcpStream::connect(addr)?;

    let write = stream.clone();
    let sender = spawn_link!(|write, mailbox: Mailbox<Message>| {
        loop {
            // TODO: I should probably just close the loop in case of some specific errors, like for
            // example a problem with writing to the stream, but at the moment it's just a catch all,
            // cause I don't have time
            if let Err(e) = process_write_loop(&mut write, &mailbox) {
                println!("Got an error while processing the write loop: {e:?}");
            }
        }
    });

    Ok((sender, stream))
}

#[cfg(feature = "async")]
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

#[cfg(feature = "async")]
#[derive(Debug)]
struct RegisterListener {
    respond_to: oneshot::Sender<String>,
    message_id: Uuid,
}

#[cfg(feature = "async")]
#[derive(Debug)]
enum InternalMessage {
    RegisterListener(RegisterListener),
}

#[cfg(feature = "async")]
pub struct Client {
    sender: UnboundedSender<Message>,
    internal_sender: UnboundedSender<InternalMessage>,
    close_receiver: Option<oneshot::Receiver<()>>,
}

#[cfg(feature = "async")]
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

#[cfg(feature = "async")]
pub trait Service<DummyType>: Send + Sync {
    type Response: Send + Serialize;
    type Request: DeserializeOwned + Send;

    fn handle_request(
        &mut self,
        message: Self::Request,
    ) -> Pin<Box<dyn Future<Output = Self::Response> + Send + '_>>;
}

#[cfg(feature = "lunatic")]
use lunatic::{
    net::{self, TcpStream},
    spawn_link, Mailbox, MessageSignal, Process, ProcessDiedSignal,
};

#[cfg(feature = "lunatic")]
use lunatic_message_request::{MessageRequest, ProcessRequest};

#[cfg(feature = "lunatic")]
pub trait Service<DummyType> {
    type Response: Serialize;
    type Request: DeserializeOwned;

    fn handle_request(&mut self, message: Self::Request) -> Self::Response;
}

#[cfg(feature = "lunatic")]
struct Client {
    sender: Process<Message>,
    internal_sender: Process<InternalMessage>,
    mailbox: Mailbox<String>,
    receiver: Process<Message>,
    close_mailbox: Mailbox<std::string::String, lunatic::serializer::Bincode, ProcessDiedSignal>,
}

#[cfg(feature = "lunatic")]
#[derive(Serialize, Deserialize, Debug)]
struct RegisterListener {
    message_id: Uuid,
    respond_to: Process<String>,
}

#[cfg(feature = "lunatic")]
#[derive(Serialize, Deserialize)]
enum InternalMessage {
    RegisterListener((Uuid, Process<String>)),
    RemoveListener(MessageRequest<Uuid, Process<String>>),
}

#[cfg(feature = "lunatic")]
impl Client {
    pub fn new<T, DummyType>(
        sender: Process<Message>,
        service: T,
        mailbox: Mailbox<String>,
        stream: lunatic::net::TcpStream,
    ) -> Self
    where
        T: Service<DummyType> + Serialize + DeserializeOwned,
        <T as Service<DummyType>>::Request: Serialize + DeserializeOwned,
        <T as Service<DummyType>>::Response: Serialize + DeserializeOwned,
    {
        let close_mailbox = mailbox.monitorable();
        close_mailbox.monitor(sender);

        let internal_sender = spawn_link!(|mailbox: Mailbox<InternalMessage>| {
            let mut listeners: HashMap<Uuid, Process<String>> = Default::default();

            loop {
                let message = mailbox.receive();
                match message {
                    InternalMessage::RegisterListener(listener) => {
                        listeners.insert(listener.0, listener.1);
                    }
                    InternalMessage::RemoveListener(req) => {
                        let uuid = req.deref();
                        let reply = listeners.remove(uuid).unwrap();
                        req.reply(reply);
                    }
                };
            }
        });

        let args = (service, sender, internal_sender.clone());
        let message_handle = spawn_link!(|args, mailbox: Mailbox<Message>| {
            let (service, mut sender, internal_sender) = args;
            let mut service = service;
            loop {
                let message = mailbox.receive();
                if let Some(reply_to) = message.reply_to {
                    let listener =
                        internal_sender.request(InternalMessage::RemoveListener, reply_to);

                    listener.send(message.message);
                } else {
                    let deserialized = serde_json::from_str::<<T as Service<DummyType>>::Request>(
                        &message.message,
                    )
                    .unwrap();
                    let response = service.handle_request(deserialized);

                    let reply = Message {
                        id: Uuid::new_v4(),
                        reply_to: Some(message.id),
                        message: serde_json::to_string(&response).unwrap(),
                        message_type: std::any::type_name::<T>().to_string(),
                    };

                    sender.send(reply);
                }
            }
        });

        let args = (stream, message_handle);
        spawn_link!(|args, m: Mailbox<()>| {
            let (mut read, message_handle) = args;
            loop {
                let mut length_buffer = [0u8; 4];
                read.read(&mut length_buffer).unwrap();
                let len = as_u32_be(&length_buffer) as usize;
                if len > 0 {
                    let mut message_buffer = vec![0u8; len];

                    if read.read_exact(&mut message_buffer).is_err() {
                        break;
                    }

                    let message = String::from_utf8(message_buffer).unwrap();
                    let message: Message = serde_json::from_str(&message).unwrap();
                    message_handle.send(message);
                }
            }
        });

        Self {
            sender,
            internal_sender,
            mailbox,
            receiver: message_handle,
            close_mailbox,
        }
    }

    pub fn request<
        T: Serialize + std::fmt::Debug + DeserializeOwned,
        Y: Serialize + std::fmt::Debug + DeserializeOwned,
    >(
        &self,
        message: T,
    ) -> anyhow::Result<Y> {
        let message = Message {
            id: Uuid::new_v4(),
            reply_to: None,
            message: serde_json::to_string(&message).unwrap(),
            message_type: std::any::type_name::<T>().to_string(),
        };

        let mailbox = self.mailbox.monitorable();
        let process = Process::spawn(mailbox.this(), reply_to_process);
        mailbox.monitor(process);

        let register_data = (message.id, process);
        self.internal_sender
            .send(InternalMessage::RegisterListener(register_data));
        self.sender.send(message);

        loop {
            match mailbox.receive() {
                MessageSignal::Message(reply) => {
                    mailbox.stop_monitoring(process);
                    return Ok(serde_json::from_str(&reply)?);
                }
                MessageSignal::Signal(ProcessDiedSignal(id)) => {
                    println!("Process {id} died");
                    return Err(anyhow::anyhow!("Didn't get a reply"));
                }
            }
        }
    }

    fn wait(&mut self) {
        loop {
            match self.close_mailbox.receive() {
                MessageSignal::Signal(ProcessDiedSignal(_)) => {
                    break;
                }
                _ => { }
            }
        }
    }
}

#[cfg(feature = "lunatic")]
fn reply_to_process(parent: Process<String>, mailbox: Mailbox<String>) {
    let message = mailbox.receive();
    parent.send(message);
    // TODO: I totally don't like it. we can't exit this process until the parent process processes
    // the reply, not sure how to do it better at the moment
    lunatic::sleep(std::time::Duration::from_secs(1));
}

#[cfg(feature = "lunatic")]
fn as_u32_be(array: &[u8; 4]) -> u32 {
    ((array[0] as u32) << 24)
        + ((array[1] as u32) << 16)
        + ((array[2] as u32) << 8)
        + ((array[3] as u32) << 0)
}
