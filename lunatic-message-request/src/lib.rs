//! **Message responses using regular [lunatic] processes.**
//!
//! Regular lunatic processes don't typically support sending responses in messages.
//! This library provides a type [`MessageRequest`] which adds support for replying back with a value.
//!
//! Interally, it uses a [Tag](lunatic::Tag) to create a temporary mailbox which waits for a message on the generated request tag.
//!
//! [lunatic]: https://crates.io/crates/lunatic
//!
//! # Example
//!
//! ```
//! use lunatic::spawn_link;
//! use lunatic_message_request::{MessageRequest, ProcessRequest};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize)]
//! enum Message {
//!     Inc,
//!     Dec,
//!     Count(MessageRequest<(), i32>),
//! }
//!
//! let counter = spawn_link!(|mailbox: Mailbox<Message>| {
//!     let mut count = 0;
//!
//!     loop {
//!         let msg = mailbox.receive();
//!         match msg {
//!             Message::Inc => count += 1,
//!             Message::Dec => count -= 1,
//!             Message::Count(req) => req.reply(count),
//!         }
//!     }
//! });
//!
//! // Initial count should be 0
//! let count = counter.request(Message::Count, ());
//! assert_eq!(count, 0);
//!
//! // Increment count
//! counter.send(Message::Inc);
//!
//! // Count should now be 1
//! let count = counter.request(Message::Count, ());
//! assert_eq!(count, 1);
//! ```

#![warn(missing_docs)]

mod message_request;
mod tagged_mailbox;

pub use message_request::*;
pub use tagged_mailbox::*;
