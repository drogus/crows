use std::{ops, time::Duration};

use lunatic::{
    serializer::{Bincode, CanSerialize},
    MailboxResult, Process, Tag,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::TaggedMailbox;

/// Message containing a request, allowing for a response to be sent back.
///
/// Message requests can be created using [`MessageRequest::new`].
///
/// **A message request should always have [`reply`](MessageRequest::reply) called,
/// otherwise a process could be left waiting forever for a response (unless receive_timeout is used).**
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "Req: Serialize + for<'dee> Deserialize<'dee>")]
#[must_use]
pub struct MessageRequest<Req, Res, S = Bincode>
where
    S: CanSerialize<Res>,
{
    req: Req,
    req_process: Process<Res, S>,
    tag: Tag,
}

impl<Req, Res, S> MessageRequest<Req, Res, S>
where
    S: CanSerialize<Res>,
{
    /// Creates a new [MessageRequest] and [TaggedMailbox].
    ///
    /// The tagged mailbox can be used to wait for a response using [TaggedMailbox::receive].
    ///
    /// Internally, this method creates a tag which is shared between the message request and tagged mailbox.
    pub fn new(req: Req) -> (Self, TaggedMailbox<Res, S>) {
        let tag = Tag::new();
        let req = Self {
            req,
            req_process: unsafe { Process::this() },
            tag,
        };
        let mailbox = TaggedMailbox::from_tag(tag);

        (req, mailbox)
    }

    /// Splits the message request into a [PendingProcess] and the inner request.
    ///
    /// This is useful if you need ownership of the inner request directly.
    pub fn into_parts(self) -> (PendingProcess<Res, S>, Req) {
        (
            PendingProcess {
                process: self.req_process,
                tag: self.tag,
            },
            self.req,
        )
    }

    /// Replies to the process with a response.
    pub fn reply(self, res: Res)
    where
        Res: Serialize + DeserializeOwned,
    {
        self.req_process.tag_send(self.tag, res);
    }
}

impl<Req, Res, S> ops::Deref for MessageRequest<Req, Res, S>
where
    S: CanSerialize<Res>,
{
    type Target = Req;

    fn deref(&self) -> &Self::Target {
        &self.req
    }
}

impl<Req, Res, S> ops::DerefMut for MessageRequest<Req, Res, S>
where
    S: CanSerialize<Res>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.req
    }
}

/// A process waiting to receive a response.
#[derive(Serialize, Deserialize)]
pub struct PendingProcess<M, S = Bincode> {
    process: Process<M, S>,
    tag: Tag,
}

impl<M, S> PendingProcess<M, S> {
    /// Returns a local node process ID.
    pub fn id(&self) -> u64 {
        self.process.id()
    }

    /// Returns a node ID.
    pub fn node_id(&self) -> u64 {
        self.process.node_id()
    }

    /// Replies to the request process with a response.
    pub fn reply(self, res: M)
    where
        S: CanSerialize<M>,
    {
        self.process.tag_send(self.tag, res);
    }

    /// Returns the inner process.
    pub fn process(&self) -> Process<M, S> {
        self.process
    }

    /// Returns the tag used by the pending process.
    pub fn tag(&self) -> Tag {
        self.tag
    }
}

/// Trait for sending a message request directly on a lunatic process.
pub trait ProcessRequest<M, S = Bincode> {
    /// Sends a message request and waits for a reponse.
    fn request<Res, Req, F>(&self, f: F, req: Req) -> Res
    where
        F: Fn(MessageRequest<Req, Res, S>) -> M,
        S: CanSerialize<Res> + CanSerialize<M>;

    // Sends a message request and waits for a reponse with a timeout.
    // fn request_timeout<Res, Req, F>(&self, f: F, req: Req, timeout: Duration) -> MailboxResult<Res>
    // where
    //     F: Fn(MessageRequest<Req, Res, S>) -> M,
    //     S: CanSerialize<Res> + CanSerialize<M>;
}

impl<M, S, PS> ProcessRequest<M, S> for Process<M, PS>
where
    PS: CanSerialize<M>,
{
    fn request<Res, Req, F>(&self, f: F, req: Req) -> Res
    where
        F: Fn(MessageRequest<Req, Res, S>) -> M,
        S: CanSerialize<Res> + CanSerialize<M>,
    {
        let (msg_req, mailbox) = MessageRequest::new(req);
        let msg = f(msg_req);
        self.send(msg);
        mailbox.receive()
    }

    // fn request_timeout<Res, Req, F>(&self, f: F, req: Req, timeout: Duration) -> MailboxResult<Res>
    // where
    //     F: Fn(MessageRequest<Req, Res, S>) -> M,
    //     S: CanSerialize<Res> + CanSerialize<M>,
    // {
    //     let (msg_req, mailbox) = MessageRequest::new(req);
    //     let msg = f(msg_req);
    //     self.send(msg);
    //     mailbox.receive_timeout(timeout)
    // }
}

#[cfg(test)]
mod tests {
    use lunatic::{spawn_link, test};
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Serialize, Deserialize)]
    enum Message {
        Inc,
        Dec,
        Count(MessageRequest<(), i32>),
    }

    #[test]
    fn counter() {
        let process = spawn_link!(|mailbox: Mailbox<Message>| {
            let mut count = 0;

            loop {
                let msg = mailbox.receive();
                match msg {
                    Message::Inc => count += 1,
                    Message::Dec => count -= 1,
                    Message::Count(req) => req.reply(count),
                }
            }
        });

        let count = process.request(Message::Count, ());
        assert_eq!(count, 0);

        process.send(Message::Inc);

        let count = process.request(Message::Count, ());
        assert_eq!(count, 1);

        process.send(Message::Inc);

        let count = process.request(Message::Count, ());
        assert_eq!(count, 2);

        process.send(Message::Dec);

        let count = process.request(Message::Count, ());
        assert_eq!(count, 1);
    }

    #[test]
    fn timeout() {
        let process = spawn_link!(|mailbox: Mailbox<Message>| {
            let mut count = 0;

            loop {
                let msg = mailbox.receive();
                match msg {
                    Message::Inc => count += 1,
                    Message::Dec => count -= 1,
                    Message::Count(req) => {
                        lunatic::sleep(Duration::from_millis(100));
                        req.reply(count);
                    }
                }
            }
        });

        let count = process.request_timeout(Message::Count, (), Duration::from_millis(200));
        assert_eq!(count.unwrap(), 0);

        process.send(Message::Inc);

        let count = process.request_timeout(Message::Count, (), Duration::from_millis(50));
        assert!(count.is_timed_out());
    }
}
