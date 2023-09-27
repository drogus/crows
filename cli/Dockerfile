FROM rust:1.72-buster as builder

WORKDIR /usr/src/app

RUN apt-get update
RUN apt-get install -y protobuf-compiler

COPY . .

RUN cd cli && cargo build --release

FROM debian:buster

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl1.1 \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /app

COPY --from=builder /usr/src/app/target/release/cli /app/app

WORKDIR /app

CMD ["/app/app"]
