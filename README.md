## Crows

> Release a murder of crows upon your enemies.

Crows is a distributed load and stress testing runner. The tests can be written in any language that can compile to WASM given the bindings for the library are available. At the moment the bindings are available only for Rust, but once the ABIs are stable it should be relatively straightforward to add more languages.

### Showcase

A sample scenario written in Rust looks like this:

```
#[config]
fn config() -> ExecutorConfig {
    let config = ConstantArrivalRateConfig {
        duration: Duration::from_secs(5),
        rate: 10,
        allocated_vus: 10,
        ..Default::default()
    };
    ExecutorConfig::ConstantArrivalRate(config)
}

#[export_name = "scenario"]
pub fn scenario() {
    http_request(
        "https://google.com".into(), GET, HashMap::new(), "".into(),
    );
}
```

It will send 10 requests per second to google.com. For information on how to compile and run it please go to the [Usage section](#usage)

### State of the project

This project is at a "working proof of concept" stage. It has solid foundation as I put a lot of thought into figuring out the best way to implement it, but there are a lot of details that are either missing or are knowingly implemented in a not optimal/best way. I've started thinking about the project about 3 years ago and I've started the current iteration about 1.5 years ago and I decided I have to finish it as soon as possible or it will end up where most of my personal projects - aiming for perfection, half finished and never released.

What you should expect then? Most of the basic stuff works, but error handling is quite poor. Stuff will probably break here and there. Error messages may make no sense or may not even reach the CLI if they happen on the server side. Also, there is only one excutor: constant arrival rate. Use if you're curious and/or you would like to contribute, but it's probably not the best idea to use it for anything important or if you value your time. I will update the README once I feel the project is stable enough and feature complete enough for a usable version.

### Why crows?

There are a lot of tools for stress testing available on the market. Why have I decided to write Crows then? A few reasons:

* I've struggled with setting up pretty much any distributed testing tool I've tried in a reasonable amount of time.
  It might be a skill issue, but it's not the worst reason to start a project I guess
* I'm very excited by the concept of using WASM on the backend for several reasons and I think
  it's interesting to try it out in context of load testing
* I think a notion of using your favourite programming language for writing stress tests while being able to
  reuse the foundation is very tempting. Of course most programmers will be able to write a Javascript load
  test even if they mostly use Ruby or Python in their day job, but isn't it nice to not have to switch languages?
  * if anyone is wondering why I mention dynamic languages that don't directly compile to WebAssembly - it is
    possible to run Ruby, Python or JavaScript based scripts on top of WebAssembly, but it requires a bit more work.
    It's something I would definitely want to explore
* I just had an itch

Crows is not production ready, so it misses some of the features, but my personal high level wishlist is:

* almost no setup, deploy some docker images and that's it!
* support for multipe languages
* run faster than any other stress testing tool out there
* optional integration with infra tools so that workers can be automatically scaled up and down as needed
* seamless distributed mode
* out of the box support for metrics and logs
* out of the box support for WebSockets and GRPC
* plugin system (probably based on WASM) to allow extending the tool without necessarily extending the source
* a web interface
* a nice TUI (Terminal UI)

### Installing

At the moment you can install the `crows` only through the `cargo` command:

```
cargo install crows
```
 
### Usage

#### Local mode

The simplest way to use Crows is to use the local mode. After you obtain the `crows` binary you can run a scenario compiled to WebAssembly with a single command:

```
crows run scenario.wasm
```

In order to create a scenarios you have to generate a Rust appliction with:

```
cargo new crows-sample
```

Then you need to set the crate type and add the `crows-bindings` dependency:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
crows-bindings = "0.3"
```

then add your scenario in `src/lib.rs`:

```rust
use crows_bindings::{
    config, http_request, ConstantArrivalRateConfig, ExecutorConfig, HTTPMethod::*,
};
use std::time::Duration;

#[config]
fn config() -> ExecutorConfig {
    let config = ConstantArrivalRateConfig {
        duration: Duration::from_secs(5),
        rate: 10,
        ..Default::default()
    };
    ExecutorConfig::ConstantArrivalRate(config)
}

#[export_name = "scenario"]
pub fn scenario() {
    println!("Send request to https://test.k6.io/");
    http_request(
        "https://test.k6.io".to_string(), GET, Default::default(), "".to_string(),
    );
}
```

I'm hoping K6 maintainers won't be angry about using their test URL. It's a great tool, btw, if you're into stress testing you should check it out!

Now you can compile the scenario with:

```
cargo build --release --target wasm32-wasi
```

and run with:

```
crows run target/wasm32-wasi/release/crows_sample.wasm
```

#### Distributed mode

In a distributed mode you need to start a coordinator and at least one worker. You interact with the coordinator using the CLI.

```
cargo install crows-coordinator
cargo install crows-worker
```

First you start the coordinator:

```
crows-coordinator
```

and then at least one worker:

```
WORKER_NAME=worker-1 crows-worker
```

Now you can upload and run a scenario:

```
crows upload --name test --path scenario.wasm
crows start --name test --workers-number 1
```

### Architecture

I plan to prepare a more thorough description with diagrams and stuff, so in here I just wanted to give a brief summary.

A high level overview is as follows:

* there is a central server that acts as bridge between the client and the workers called coordinator
* tests are executed inside workers. Workers are connected to the coordinator
* only the coordinator needs to have a network interface exposed so its reachable by the workers and the client. The connection between workers and the coordinator works both ways
* a client can connect to a coordinator to issue commands and get diagnostic information and test results

A coordinator is a single point of failure, but in the case of stress and load testing I think it's a sensible trade-off. On the upside this kind of architecture is extremely simple and with persistance layer it might be possible to tolerate coordinator crashes. In the future I'm open to making the coordinator distributed, but I don't think it's a pressing issue. Most of the issues I can identify with a single point of contact are not very problematic:

1. Crashes may be recovered from given a persistance layer, but even without that stress and load testing is typically not an uptime sensitive task
2. Although a single server orchestrating a number of workers may run out of resources, with a relatively strong server and good bandwidth I think ceiling is still very high. I plan to do stress testing of the tool itself at some point, but I would be surprised if Crows couldn't run at least a few hundred of workers.
3. WASM binaries may have a substantial size, but there are ways to optimize for size. If sending a bigger binary to hundreds of workers ever becomes a problem, though, there are ways to spread sending the data through the workers without necessarily changing them into coordinators themselves.

On top of that I really want to avoid the situation where every node can potentially be either a coordinator or a worker, cause I think it would complicate the project quite a lot without that many advantages.

#### Connections

Connections between components are plain TCPSocket connections with messages being lenght delimited JSON messages (ie. each message starts with 4 bytes describing the message length and then the message itself). The messages are abstracted with a custom RPC system that allows two way communication with either side initiating a message. In other words both sides can send a request and receive a response. One of the things that was really important for me was not having a requirement to expose the network address of a worker, cause this drastically simplifies server setup - it can be literally any machine or a virtual machine that connect to the coordinator. Even your laptop behind a NAT.

In the future I think I might change the serialization format to something like FlatBuffers, so it's easier to work with different versions, but for now JSON is enough and its simplicity makes development easier.

#### Worker

Worker is one of the most interesting parts of the project, in my humble opinion. A workers runs on top of a Tokio asynchronous runtime. All of the scenarios are executed in the context of the async runtime, thus even though the scenarios are written like you would normally write synchronous code, they will be executed in an asynchronous wait under the hood.

When a scenario is being executed a WASM runtime, using Wasmtime, is created. All the IO operations are done on the host side and thus WASM modules only call the host defined functions. For example in order to make an HTTP request:

1. The WASM module prepares an `HTTPRequest` object and writes it to WASM linear memory
2. The WASM module calls the `http_request` binding that in turn executes code in the Tokio runtime
3. In an async function the host copies the request from the WASM module linear memory and it sends an HTTP request
4. The response is saved in a buffer on the host side and the location of the buffer is returned to the WASM module
5. The WASM module requests copying the response buffer to the WASM linear memory and it converts it to either a HTTPResponse or HTTPError struct

Objects shared between the host and the WASM module are serialized and deserialized using JSON.

WASM modules are compiled as WASI enabled modules, which means it's possible to support common I/O operations like printing to STDOUT or reading files. At the moment only the former is supported and anything printed to STDOUT or STDERR will be passed to the coordinator and then potentially to the client. In the future I would also like to support reading files and allowing to distribute files between workers.

#### Executors

Executors define how scenarios are being executed. They define how and when to allocated new WASM instances and when to run iterations. At the moment only one executor is available constant arrival rate. If you want to see some other examples of possible executors take a look at (executors available in the K6 project)[https://k6.io/docs/using-k6/scenarios/executors/] (which is a great tool, which I've successfully used in the past, btw. kudos to the K6 team!).

### Roadmap/TODO

I don't have a very precise plan on where I want Crows to go, but some loose thoughts on the stuff I'd like to work on if I have time (which is scarce and you can help by supporting me (on GitHub!)[link to gh sponsors]):

1. Metrics support as at the moment it's not implemented at all
2. Support for WebSockets and gRPC. I honestly haven't thought much about these yet and I think it might be problematic, cause WASM doesn't have a good support for threads yet, but maybe callbacks could be an answer?
3. Fault tolerance/persistance - if a coordinator crashes and it gets up workers will reconnect, but any state that worker has will be lost
4. I would really like to look into WASM components. At the moment the hardest part of adding support to additional languages is writing bindings for the host functions like HTTP request. It isn't very hard at the moment as only HTTP requests are supported, but the more stuff I add, the harder it becomes. Another thing is that with WASM components support it might be feasible to support imports from other languages and thus to share tools with others
5. Extension support - similarly to the above. At the moment one can use extensions of scenario functionality in a supported language, for example it's possible to write a library for Rust, distribute it on crates.io and import in a scenario just as any other Rust crate, but extending core functionality is not supported (for example writing your own executor). It coud be also nice to extend the way scenarios are called, for exampe for custom metrics etc.
6. File support. WASI allows to support i/o. but at the moment only printing on a screen is enabled. It would be also possible to do stuff like "put a file on S3 and treat it as a regular File.read() in a test for simplicity"
7. Streaming responses. For simplicity I wait for the entire response before returning from the host. It's not necessary and the body could be returned as a stream. This could also allow for seamless "don't even read the body" support for better performance. If all you do is try to stress test a server and you don't care about responses the fastest way to do that is to ignore the body
8. For simplicity I use Reqwest HTTP library on the host, which has an internal pool of clients. I haven't given it too much thought yet, but I think that I would prefer to have a greater control over how HTTP connections are handled. For example I think it makes most sense if a scenario uses dedicated connections and is generally not reused between scenarios. It could be nice to allow sharing a connection between scenarios for performance, which is something for example `wrk` does by default between requests, but it should be configurable and in general we should have more granular control on how it works
9. Look closer into serialization formats, both for the communication between components and communication between WASM modules and the host. I would ideally want something with ability do define a schema and make backwards compatible changes to the schema.
10. More performant memory management. For example at the moment sending an HTTP request means serializing the entire request in the WASM module and then deserializing it on the host side. If a stress test requires sending big files it's generally a waste of CPU cycles as we could quite easily serialize only the headers and url, write body to the WASM memory directly and then send the body straight from the memory rather than copy it anywhere. Another example is that most of the time RPC services and clients could use references.
11. At the moment there is only one stage possible to be executed - running the scenario itself. I want to also add some kind of a "prepare" step which would run before the scenario itself and which would prepare the data, but I need to think a bit more on how exactly it should work.

### License

Crows is licensed under AGPLv3 license.

#### CLA

Before accepting contributions I would like contributors to sign a CLA. I know it may be seen as hostile to the OSS movement, especially in the light license changes of projects like Terraform or Redis. I don't plan any moves like that, but I'm also a single person without any budget and with responsibilities towards my family, so I would like to leave myself some options. The most probable scenario where I might need a CLA is if I want also sell a commercial license for companies that don't want to to use an AGPLv3 tool for any reason or to start a hosted paid version of the tool where you can register and run your scenarios if you don't want to host the tool yourself. Both things are equally unlikely at the moment, but you never know.
