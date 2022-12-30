FROM gcr.io/distroless/static:nonroot
LABEL org.opencontainers.image.source=https://github.com/kube-rs/controller-rs
LABEL org.opencontainers.image.description="Kube Example Controller"
LABEL org.opencontainers.image.licenses="Apache-2.0"
COPY --chown=nonroot:nonroot ./controller /app/
EXPOSE 8080
ENTRYPOINT ["/app/controller"]
