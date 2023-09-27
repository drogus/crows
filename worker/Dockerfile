FROM rust:1.72-buster as builder

WORKDIR /app

RUN apt-get update
RUN apt-get install -y protobuf-compiler

COPY . .

RUN cd worker && cargo build --release

RUN cargo install lunatic-runtime
RUN cp `which lunatic` /app/lunatic

FROM debian:buster

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl1.1 \
        sqlite3 \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /app

COPY --from=builder /app/worker/target/wasm32-wasi/release/worker.wasm /app/worker.wasm
COPY --from=builder /app/lunatic /app/lunatic

WORKDIR /app

ENV WORKER_NAME=$HOST

ENTRYPOINT ["sh", "-c", "/app/lunatic worker.wasm"]
CMD ["127.0.0.1:8181", "$WORKER_NAME"]
