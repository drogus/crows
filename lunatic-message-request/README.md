**Message responses using regular [lunatic] processes.**

Regular lunatic processes don't typically support sending responses in messages.
This library provides a type `MessageRequest` which adds support for replying back with a value.

# Example

```rust
use lunatic::spawn_link;
use lunatic_message_request::{MessageRequest, ProcessRequest};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
enum Message {
    Inc,
    Dec,
    Count(MessageRequest<(), i32>),
}

let counter = spawn_link!(|mailbox: Mailbox<Message>| {
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

// Initial count should be 0
let count = counter.request(Message::Count, ());
assert_eq!(count, 0);

// Increment count
counter.send(Message::Inc);

// Count should now be 1
let count = counter.request(Message::Count, ());
assert_eq!(count, 1);
```

## License

Licensed under either

- [Apache License 2.0]
- [MIT License]

at your option.

[lunatic]: https://crates.io/crates/lunatic
[Apache License 2.0]: ./LICENSE-APACHE
[MIT License]: ./LICENSE-MIT
