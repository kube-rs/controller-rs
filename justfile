NAME := "controller"
ORG := "kube-rs"
VERSION := `git rev-parse HEAD`
SEMVER_VERSION := `grep version Cargo.toml | awk -F"\"" '{print $2}' | head -n 1`

default:
  @just --list --unsorted --color=always | rg -v "    default"

# install crd into the cluster
install-crd: generate
  kubectl apply -f yaml/crd.yaml

generate:
  cargo run --bin crdgen > yaml/crd.yaml
  helm template charts/doc-controller > yaml/deployment.yaml

# run with opentelemetry
run-telemetry:
  OPENTELEMETRY_ENDPOINT_URL=https://0.0.0.0:55680 RUST_LOG=info,kube=debug,controller=debug cargo run --features=telemetry

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
  docker build -t {{ORG}}/{{NAME}}:{{VERSION}} .

# retag the current git versioned docker tag as latest, and publish both
tag-latest:
  docker tag {{ORG}}/{{NAME}}:{{VERSION}} {{ORG}}/{{NAME}}:latest
  docker push {{ORG}}/{{NAME}}:{{VERSION}}
  docker push {{ORG}}/{{NAME}}:latest

# retag the current git versioned docker tag as the current semver and publish
tag-semver:
  #!/usr/bin/env bash
  if curl -sSL https://registry.hub.docker.com/v1/ORGsitories/{{ORG}}/{{NAME}}/tags | jq -r ".[].name" | grep -q {{SEMVER_VERSION}}; then
    echo "Tag {{SEMVER_VERSION}} already exists - not publishing"
  else
    docker tag {{ORG}}/{{NAME}}:{{VERSION}} {{ORG}}/{{NAME}}:{{SEMVER_VERSION}} .
    docker push {{ORG}}/{{NAME}}:{{SEMVER_VERSION}}
  fi

# local helpers for debugging traces

# forward grpc otel port from svc/promstack-tempo in monitoring
forward-tempo:
  kubectl port-forward -n monitoring svc/promstack-tempo 55680:55680

# forward http port from svc/promstack-grafana in monitoring
forward-grafana:
  kubectl port-forward -n monitoring svc/promstack-grafana 8000:80

# mode: makefile
# End:
# vim: set ft=make :
