## Crows

Crows is a distributed load and stress testing runner. The tests can be written in languages that are supported by a [Lunatic runtime](https://lunatic.solutions/). At this point it means Rust and AssemblyScript.

Testing scenarios are compiled to WASM modules and thus are easy to manage. No need to rebuild docker images or redeploy lambda functions in order to fix a problem with a test.

### Why crows?

There are a lot of tools for stress testing available on the market, but all of them have some flaws that made it hard for me to use them the way I wanted to. After working on stress testing various workloads over the years I developed a list of features that I wish my ideal stress testing tool would have:

* easy setup - no requirement for configuration files, no fuss with setting up everything, just deploy the tool and allow adding workers even using a laptop
* distributed stress testing baked in - when you are stress testing a small web app you likely don't need much load and one server is enough. But what if you need to test millions of clients? Or millions of requests per second? Not only CPU saturation might be problematic, but also you may run into bandwidth problems. Thus running from multiple machines should be as easy as running from one machine
* allow to write scenarios in a programming language and execute them dynamically - no static-file scenarios
* out of the box support for WebSockets

Crows aims to solve hit all of these points in the future.

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

Running locally is possible if you have Rust installed. On top of that you need the `lunatic-runtime`:

```
cargo install lunatic-runtime
```

Then you can run using cargo, for example:

```
cargo run -p coordinator
cargo run -p worker -- 127.0.0.1 worker-1
cargo run -p worker -- 127.0.0.1 worker-2
cargo run -p cli -- workers list
```

