FROM rust:1.72-buster as builder

WORKDIR /app

RUN apt-get update
RUN apt-get install -y protobuf-compiler

COPY . .

RUN cd worker && cargo build --release

FROM debian:buster

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl1.1 \
        sqlite3 \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /app

COPY --from=builder /app/target/release/crows-worker /app/crows-worker

WORKDIR /app

ENV WORKER_NAME=$HOST

ENTRYPOINT ["/app/crows-worker"]
