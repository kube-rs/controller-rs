FROM clux/muslrust:stable AS planner
RUN cargo install cargo-chef
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM clux/muslrust:stable AS cacher
RUN cargo install cargo-chef
COPY --from=planner /volume/recipe.json recipe.json
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json

FROM clux/muslrust:stable AS builder
COPY . .
COPY --from=cacher /volume/target target
COPY --from=cacher /root/.cargo /root/.cargo
RUN cargo build --release --bin controller && \
    mv /volume/target/x86_64-unknown-linux-musl/release/controller .

FROM cgr.dev/chainguard/static
LABEL org.opencontainers.image.source=https://github.com/kube-rs/controller-rs
LABEL org.opencontainers.image.description="Kube Example Controller"
LABEL org.opencontainers.image.licenses="Apache-2.0"
COPY --from=builder --chown=nonroot:nonroot /volume/controller /app/
EXPOSE 8080
ENTRYPOINT ["/app/controller"]
