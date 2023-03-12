FROM clux/muslrust:stable AS builder
COPY . .
RUN --mount=type=cache,target=/volume/target \
    --mount=type=cache,target=/root/.cargo/registry \
    cargo build --release --bin controller && \
    mv /volume/target/x86_64-unknown-linux-musl/release/controller .

FROM cgr.dev/chainguard/static
LABEL org.opencontainers.image.source=https://github.com/kube-rs/controller-rs
LABEL org.opencontainers.image.description="Kube Example Controller"
LABEL org.opencontainers.image.licenses="Apache-2.0"
COPY --from=builder --chown=nonroot:nonroot /volume/controller /app/
EXPOSE 8080
ENTRYPOINT ["/app/controller"]
