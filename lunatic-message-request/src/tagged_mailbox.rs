use std::time::Duration;

use lunatic::{
    serializer::{Bincode, CanSerialize},
    Mailbox, MailboxResult, Tag,
};
use serde::{Deserialize, Serialize};

/// A lunatic mailbox with a tag.
///
/// This is useful for receiving messages with the given tag,
/// even if the current process does not take `M` message type.
#[derive(Clone, Copy, Debug)]
pub struct TaggedMailbox<M, S = Bincode, L = ()>
where
    S: CanSerialize<M>,
{
    tag: Tag,
    mailbox: Mailbox<M, S, L>,
}

impl<M, S, L> TaggedMailbox<M, S, L>
where
    S: CanSerialize<M>,
{
    /// Creates a new tag and mailbox, returning a [TaggedMailbox].
    pub fn new() -> Self {
        TaggedMailbox {
            tag: Tag::new(),
            mailbox: unsafe { Mailbox::new() },
        }
    }

    /// Creates a [TaggedMailbox] from an existing [Tag].
    pub fn from_tag(tag: Tag) -> Self {
        TaggedMailbox {
            tag,
            mailbox: unsafe { Mailbox::new() },
        }
    }

    /// Returns the inner tag.
    pub fn tag(&self) -> Tag {
        self.tag
    }

    // Receives a message with a timeout.
    // pub fn receive_timeout(&self, timeout: Duration) -> MailboxResult<M> {
    //     self.mailbox.tag_receive_timeout(&[self.tag], timeout)
    // }
}

impl<M, S> TaggedMailbox<M, S, ()>
where
    S: CanSerialize<M>,
{
    /// Receives a message.
    pub fn receive(&self) -> M {
        self.mailbox.tag_receive(&[self.tag])
    }
}

impl<M, S, L> Default for TaggedMailbox<M, S, L>
where
    S: CanSerialize<M>,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<M, S, L> PartialEq for TaggedMailbox<M, S, L>
where
    S: CanSerialize<M>,
{
    fn eq(&self, other: &Self) -> bool {
        self.tag == other.tag
    }
}

impl<M, S, L> Eq for TaggedMailbox<M, S, L> where S: CanSerialize<M> {}

impl<M, Se, L> Serialize for TaggedMailbox<M, Se, L>
where
    Se: CanSerialize<M>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.tag.serialize(serializer)
    }
}

impl<'de, M, S, L> Deserialize<'de> for TaggedMailbox<M, S, L>
where
    S: CanSerialize<M>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let tag = Tag::deserialize(deserializer)?;
        Ok(TaggedMailbox {
            tag,
            mailbox: unsafe { Mailbox::new() },
        })
    }
}
