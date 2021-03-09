FROM rust:1.50 as builder
WORKDIR /usr/src/controller
RUN apt-get update && apt-get install cmake protobuf-compiler -y
COPY . .
RUN cargo install --locked --path .

FROM debian:buster-slim
RUN apt-get update && apt-get install -y openssl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/controller /usr/local/bin/controller
CMD ["controller"]
