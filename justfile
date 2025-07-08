[private]
default:
  @just --list --unsorted

# install crd into the cluster
install-crd: generate
  kubectl apply -f yaml/crd.yaml

generate:
  cargo run --bin crdgen > yaml/crd.yaml
  helm template charts/doc-controller > yaml/deployment.yaml

# run with opentelemetry
run-telemetry:
  OPENTELEMETRY_ENDPOINT_URL=http://127.0.0.1:4317 RUST_LOG=info,kube=debug,controller=debug cargo run --features=telemetry

# run without opentelemetry
run:
  RUST_LOG=info,kube=debug,controller=debug cargo run

# format with nightly rustfmt
fmt:
  cargo +nightly fmt

# run unit tests
test-unit:
  cargo test
# run integration tests
test-integration: install-crd
  cargo test -- --ignored
# run telemetry tests
test-telemetry:
  OPENTELEMETRY_ENDPOINT_URL=http://127.0.0.1:4317 cargo test --lib --all-features -- get_trace_id_returns_valid_traces --ignored

# compile for musl (for docker image)
compile features="":
  #!/usr/bin/env bash
  docker run --rm \
    -v cargo-cache:/root/.cargo \
    -v $PWD:/volume \
    -w /volume \
    -t clux/muslrust:stable \
    cargo build --release --features={{features}} --bin controller
  cp target/x86_64-unknown-linux-musl/release/controller .

[private]
_build features="":
  just compile {{features}}
  docker build -t clux/controller:local .

# docker build base
build-base: (_build "")
# docker build with telemetry
build-otel: (_build "telemetry")


# local helper for test-telemetry and run-telemetry
# forward grpc otel port from svc/promstack-tempo in monitoring
forward-tempo:
  kubectl port-forward -n monitoring svc/promstack-tempo 4317:4317
