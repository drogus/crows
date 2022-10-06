use futures::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_serde::formats::*;
use tokio_util::codec::FramedRead;
use tokio_util::codec::{FramedWrite, LengthDelimitedCodec};

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub enum Message {
    Foo,
    Bar,
}

pub async fn server() {
    // Bind a server socket
    let listener = TcpListener::bind("127.0.0.1:17653").await.unwrap();

    println!("listening on {:?}", listener.local_addr());

    loop {
        let (socket, _) = listener.accept().await.unwrap();
        let (reader, writer) = socket.into_split();

        // Delimit frames using a length header
        let length_delimited = FramedRead::new(reader, LengthDelimitedCodec::new());

        // Deserialize frames
        let mut deserialized = tokio_serde::SymmetricallyFramed::new(
            length_delimited,
            SymmetricalJson::<Value>::default(),
        );

        let length_delimited = FramedWrite::new(writer, LengthDelimitedCodec::new());

        let mut serialized = tokio_serde::SymmetricallyFramed::new(
            length_delimited,
            SymmetricalJson::<Message>::default(),
        );

        serialized.send(Message::Foo).await.unwrap();

        // Spawn a task that prints all received messages to STDOUT
        tokio::spawn(async move {
            while let Some(msg) = deserialized.try_next().await.unwrap() {
                println!("GOT: {:?}", msg);
            }
        });
    }
}

async fn client() {
    // Bind a server socket
    let socket = TcpStream::connect("127.0.0.1:17653").await.unwrap();

    // Delimit frames using a length header
    let length_delimited = FramedWrite::new(socket, LengthDelimitedCodec::new());

    // Serialize frames with JSON
    let mut serialized =
        tokio_serde::SymmetricallyFramed::new(length_delimited, SymmetricalJson::default());

    // Send the value
    serialized
        .send(json!({
            "name": "John Doe",
            "age": 43,
            "phones": [
                "+44 1234567",
                "+44 2345678"
            ]
        }))
        .await
        .unwrap()
}

#[tokio::main]
pub async fn main() {}
