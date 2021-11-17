FROM clux/muslrust:stable AS builder
COPY Cargo.* .
COPY src src
RUN --mount=type=cache,target=/volume/target \
    --mount=type=cache,target=/root/.cargo/registry \
    cargo build --release --bin controller && \
    mv /volume/target/x86_64-unknown-linux-musl/release/controller .

FROM gcr.io/distroless/static:nonroot
COPY --from=builder --chown=nonroot:nonroot /volume/controller /app/
EXPOSE 8080
ENTRYPOINT ["/app/controller"]
