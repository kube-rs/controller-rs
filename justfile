NAME := "controller"
ORG := "kube-rs"
VERSION := `git rev-parse HEAD`
SEMVER_VERSION := `rg '^version = "(\S*)"' -r '$1' Cargo.toml | head -n 1`

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
# run telemetry tests
test-telemetry:
  OPENTELEMETRY_ENDPOINT_URL=https://0.0.0.0:55680 cargo test --lib --all-features -- get_trace_id_returns_valid_traces --ignored

[private]
_build features="":
  DOCKER_BUILDX=1 docker build -t {{ORG}}/{{NAME}}:{{VERSION}} --build-arg FEATURES={{features}} .

# Build with default features
build: (_build "")
# Build with telemetry
build-telemetry: (_build "telemetry")


# local helpers for debugging traces

# forward grpc otel port from svc/promstack-tempo in monitoring
forward-tempo:
  kubectl port-forward -n monitoring svc/promstack-tempo 55680:55680

# forward http port from svc/promstack-grafana in monitoring
forward-grafana:
  kubectl port-forward -n monitoring svc/promstack-grafana 8000:80

# generate rbac using audit2rbac
gen-rbac:
  #!/usr/bin/env bash
  set -euxo pipefail
  cat << EOF > audit.yaml
  kind: "Policy"
  apiVersion: "audit.k8s.io/v1"
  rules:
  - level: Metadata
    users:
    - system:admin
    - system:serviceaccount:default:doc-controller
    omitStages:
    - RequestReceived
    - ResponseStarted
    - Panic
  EOF
  mkdir -p audit
  rm -f audit/audit.log
  k3d cluster create auditrbac \
    --k3s-arg '--kube-apiserver-arg=audit-policy-file=/var/lib/rancher/k3s/server/manifests/audit.yaml@server:*' \
    --k3s-arg '--kube-apiserver-arg=audit-log-path=/var/log/kubernetes/audit/audit.log@server:*' \
    --volume "$(pwd)/audit.yaml:/var/lib/rancher/k3s/server/manifests/audit.yaml" \
    --volume "$(pwd)/audit:/var/log/kubernetes/audit"
  export KUBECONFIG="$(k3d kubeconfig write auditrbac)"
  kubectl apply -f yaml/crd.yaml
  kubectl wait --for=condition=established crd/documents.kube.rs --timeout=10s
  kubectl apply -f yaml/deployment.yaml
  kubectl wait --for=condition=available deploy/doc-controller --timeout=60s
  # install stuff in multiple namespaces with multiple names
  kubectl apply -f yaml/instance-samuel.yaml
  kubectl apply -f yaml/instance-samuel.yaml -n kube-system
  kubectl apply -f yaml/instance-lorem.yaml
  sleep 1
  kubectl delete -f yaml/instance-samuel.yaml
  sleep 1
  # Needs https://github.com/liggitt/audit2rbac installed on PATH
  audit2rbac -f audit/audit.log --serviceaccount=default:doc-controller \
    --generate-labels="" --generate-annotations="" --generate-name=doc-controller

# mode: makefile
# End:
# vim: set ft=make :
