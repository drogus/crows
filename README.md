## Crows

Crows is a distributed load and stress testing runner. The tests can be written in any language that can compile to WASM given the bindings for the library are available. At the moment the bindings are available only for Rust, but once the ABIs are stable it should be relatively straight forward to add more languages.

Crows is an experimental project for the time being.

### Why crows?

There are a lot of tools for stress testing available on the market, thus a question on why should you choose Crows or why is it a good idea to write Crows in the first place, is entirely valid. As Crows is more of a proof of concept than a production ready project I don't think you should use it for reason other than curiosity, but there are certain pain points Crows is supposed to solve:

* easy setup - no requirement for configuration files and setting things up-front. The only thing required to run Crows should be a single server that is reachable to workers and a worker or multiple workers that point to the coordinator server
* distributed stress testing baked in - when you are stress testing a small web app you likely don't need much load and one server is enough. But what if you need to test millions of clients? Or millions of requests per second? Not only CPU saturation might be problematic, but also you may run into bandwidth problems or exhaust available ports for outgoing connections (roughly 64k per server per ip/port)
* allow to write scenarios in a programming language and execute them dynamically. No static-files based scenarios
* out of the box support for WebSockets and gRPC
* low footprint and high speed. Stress testing shouldn't require beefy machines
* support for multiple languages like Rust, TinyGo, AssemblyScript and potentially also JS/TS (although JavaScript doesn't compile to WASM it's possible to use WASI in order to plug JS/TS scripts into a WASM runtime)
* multipe executors. A constant arrival rate executor is a must (which surprisingly not many tools support, with a notable exception of K6)
* out of the box metrics with support of Prometheus
* out of the box logs gathering

### Usage

#### Docker

In order to use Crows with docker you can use prebuilt images. First start the coordinator, the main piece that listens for connections from workers and from the client:

```
docker run -p 8282:8282 -p 8181:8181 -t ghcr.io/drogus/crows-coordinator
```

Then let's start a worker:

```
docker run --netowrk="host" -t ghcr.io/drogus/crows-worker 127.0.0.1:8181 worker-1
```

Now running a client should give us the workers list:

```
docker run --netowrk="host" -t ghcr.io/drogus/crows-client workers list
```

The output should show `worker-1` being connected. Now let's connect another worker:

```
docker run --netowrk="host" -t ghcr.io/drogus/crows-worker 127.0.0.1:8181 worker-2
``````

The workers list should now show two workers. In order to upload a scenario you can use a compiled wasm file in the `example_module` directory. When in the repo you can run the following:

```
docker run -v $PWD/example_module/hi.wasm:/scenarios/hi.wasm --network="host" --platform linux/amd64 -t ghcr.io/drogus/crows-client upload --name hi --path /scenarios/hi.wasm
```

This will upload the `hi.wasm` binary to all of the workers with the name `hi`. Now we can run the module on the workers:

```
docker run --network="host" --platform linux/amd64 -t ghcr.io/drogus/crows-client start --name hi --concurrency 10
```

This should execute test instances on each of the workers

#### Running locally

Running locally is possible if you have Rust installed.

Then you can run using cargo, for example:

```
cargo run -p coordinator
cargo run -p worker -- 127.0.0.1 worker-1
cargo run -p worker -- 127.0.0.1 worker-2
cargo run -p cli -- workers list
```

### Architecture

I plan to prepare a more thorough description with diagrams, so in here I just wanted to give a brief summary.

A high level overview is as follows:

* there is a central server that acts as bridge between the client and the workers called coordinator
* tests are executed inside workers. Workers are connected to the coordinator
* only the coordinator needs to have a network interface exposed to the outside world. The connection between workers and the coordinator works both ways
* a client can connect to a coordinator to issue commands and get diagnostic information and reslults

A coordinator is a single point of failure, but in the case of stress and load testing I think it's a sensible trade-off. On the upside this kind of architecture is extremely simple and with persistance layer it might be possible to tolerate coordinator crashes. In the future I'm open to making the coordinator distributed, but I don't think it's a pressing issue. Most of the issues I can identify with a single point of contact are not very problematic:

1. Crashes may be recovered from given a persistance layer, but even without that stress and load testing is typically not an uptime sensitive task
2. Although a single server orchestrating a number of workers may run out of resources, with a relatively strong server and good bandwidth I think ceiling is still very high. I plan to do stress testing of the tool itself at some point, but I would be surprised if Crows couldn't run at least a few hundred of workers.
3. WASM binaries may have a substantial size, but there are ways to optimize for size. If sending a bigger binary to hundreds of workers ever becomes a problem, though, there are ways to spread sending the data through the workers without necessarily changing them into coordinators themselves.

On top of that I really want to avoid the situation where every node can potentially be either a coordinator or a worker, cause the most common solution is a complete graph, which comes with its own problems and solutions.

#### Connections

Connections between components are plain TCPSocket connections with messages being lenght delimited JSON messages (ie. each message starts with 4 bytes describing the message length and then the message itself). The messages are abstracted with a custom RPC system that allows two way communication with either side initiating a message. In other words both sides can send a request and receive a response. One of the things that was really important for me was not having a requirement to expose the network address of a worker, cause this drastically simplifies server setup - it can be literally any machine or a virtual machine that connect to the coordinator. Even your laptop behind a NAT.

In the future I think I might change the serialization format to something like FlatBuffers, so it's easier to work with different versions, but for now JSON is enough and its simplicity makes development easier.

#### Worker

Worker is one of the most interesting parts of the project, in my humble opinion. A worker runs N threads, where N by default is a number of CPUs. Each thread runs an asynchronous runtime - Tokio at the moment. As workers will mostly work on a very similar workloads I think it makes sense to use this kind of "share nothing" approach. It might be also potentially faster than running one runtime with multiple threads. I also don't rule out trying out different runtimes and I'm especially curious to try out Tokio with IO-uring.

Each of the worker threads can run multiple instances of wasmtime environments. Each wasmtime environment can run functions from the WASM module containing a scenario. All the IO operations are done on the host side and thus WASM modules only call the host defined functions. For example in order to make an HTTP request:

1. The WASM module prepares an `HTTPRequest` object and writes it to WASM linear memory
2. The WASM module calls the `http_request` binding that in turn executes code in the Tokio runtime
3. In an async function the host copies the request from the WASM module linear memory and it sends an HTTP request
4. The response is saved in a buffer on the host side and the location of the buffer is returned to the WASM module
5. The WASM module requests copying the response buffer to the WASM linear memory and it converts it to either a HTTPResponse or HTTPError struct

Objects shared between the host and the WASM module are serialized and deserialized using Borsh serialization format.

WASM modules are compiled as WASI enabled modules, which means it's possible to support common I/O operations like printing to STDOUT or reading files. At the moment only the former is supported and anything printed to STDOUT or STDERR will be passed to the coordinator and then potentially to the client. In the future I would also like to support reading files and allowing to distribute files between workers.

#### Executors

TODO

### Roadmap/TODO

I don't have a very precise plan on where I want Crows to go, but some loose thoughts on the stuff I'd like to work on if I have time (which is scarce and you can help by supporting me on Patreon or on GitHub!):

1. Metrics support as at the moment it's not implemented at all
2. Support for WebSockets and gRPC
3. Fault tolerance/persistance - if a coordinator crashes and it gets up workers will reconnect, but any state that worker has will be lost
4. I would really like to look into WASM components. At the moment the hardest part of adding support in any language is writing bindings for the host functions like HTTP request. It isn't very hard at the moment, but the more stuff I add, the harder it becomes. Another thing is that with WASM components support it might be feasible to support imports from other languages
5. Extension support - similarly to the above. At the moment one can use extensions of scenario functionality in a supported language, for example it's possible to write a library for Rust, distribute it on crates.io and import in a scenario just as any other Rust crate, but extending core functionality is not supported (for example writing your own executor). It coud be also nice to extend the way scenarios are called, for exampe for custom metrics etc.
6. File support. WASI allows to support i/o. but at the moment only printing on a screen is enabled.
7. Streaming responses. For simplicity I wait for the entire response before returning from the host. It's not necessary and the body could be returned as a stream. This could also allow for seamless "don't even read the body" support for better performance
8. For simplicity I use Reqwest HTTP library on the host, which has an internal pool of clients. I haven't given it too much thought yet, but I think that I would prefer to have a greater control over how HTTP connections are handled. For example I think it makes most sense if a scenario uses dedicated connections and is generally not reused between scenarios. It could be nice to allow sharing a connection between scenarios for performance, which is something for example `wrk` does by default between requests, but it should be configurable and in general we should have more granular control on how it works
9. Look closer into serialization formats, both for the communication between components and communication between WASM modules and the host.
10. A bit more performant memory management. For example at the moment sending an HTTP request means serializing the entire request in the WASM module and then deserializing it on the host side. If a stress test requires sending big files it's generally a waste of CPU cycles as we could quite easily serialize only the headers and url, write body to the WASM memory directly and then send the body straight from the memory rather than copy it anywhere.

### License

Crows is licensed under AGPLv3 license.
