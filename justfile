NAME := "controller"
REPO := "kube-rs"
VERSION := `git rev-parse HEAD`
SEMVER_VERSION := `grep version Cargo.toml | awk -F"\"" '{print $2}' | head -n 1`

default:
  @just --list --unsorted --color=always | rg -v "    default"

# generate and install crd into the cluster
install-crd:
  cargo run --bin crdgen > yaml/foo-crd.yaml
  kubectl apply -f yaml/foo-crd.yaml

# run with opentelemetry
run-telemetry:
  OPENTELEMETRY_ENDPOINT_URL=https://0.0.0.0:55680 RUST_LOG=info,kube=trace,controller=debug cargo run --features=telemetry

# run without opentelemetry
run:
  RUST_LOG=info,kube=trace,controller=debug cargo run

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

# docker build (requires compile step first)
build:
  docker build -t {{REPO}}/{{NAME}}:{{VERSION}} .

# retag the current git versioned docker tag as latest, and publish both
tag-latest:
  docker tag {{REPO}}/{{NAME}}:{{VERSION}} {{REPO}}/{{NAME}}:latest
  docker push {{REPO}}/{{NAME}}:{{VERSION}}
  docker push {{REPO}}/{{NAME}}:latest

# retag the current git versioned docker tag as the current semver and publish
tag-semver:
  #!/usr/bin/env bash
  if curl -sSL https://registry.hub.docker.com/v1/repositories/{{REPO}}/{{NAME}}/tags | jq -r ".[].name" | grep -q {{SEMVER_VERSION}}; then
    echo "Tag {{SEMVER_VERSION}} already exists - not publishing"
  else
    docker tag {{REPO}}/{{NAME}}:{{VERSION}} {{REPO}}/{{NAME}}:{{SEMVER_VERSION}} .
    docker push {{REPO}}/{{NAME}}:{{SEMVER_VERSION}}
  fi

# local helpers for debugging traces

# forward grpc otel port from svc/promstack-tempo in monitoring
forward-tempo:
  kubectl port-forward -n monitoring svc/promstack-tempo 55680:55680

# forward http port from svc/promstack-grafana in monitoring
forward-grafana:
  kubectl port-forward -n monitoring svc/promstack-grafana 8000:80
