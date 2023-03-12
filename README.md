## controller-rs
[![ci](https://github.com/kube-rs/controller-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/kube-rs/controller-rs/actions/workflows/ci.yml)
[![docker image](https://img.shields.io/docker/pulls/clux/controller.svg)](
https://hub.docker.com/r/clux/controller/tags/)

A rust kubernetes reference controller for a [`Document` resource](https://github.com/kube-rs/controller-rs/blob/main/yaml/crd.yaml) using [kube](https://github.com/kube-rs/kube/), with observability instrumentation.

The `Controller` object reconciles `Document` instances when changes to it are detected, writes to its `.status` object, creates associated events, and uses finalizers for guaranteed delete handling.

## Requirements
- A Kubernetes cluster / k3d instance
- The [CRD](yaml/crd.yaml)
- Opentelemetry collector (**optional**)

### Cluster
As an example; get `k3d` then:

```sh
k3d cluster create --registry-create --servers 1 --agents 1 main
k3d kubeconfig get --all > ~/.kube/k3d
export KUBECONFIG="$HOME/.kube/k3d"
```

A default `k3d` setup is fastest for local dev due to its local registry.

### CRD
Apply the CRD from [cached file](yaml/crd.yaml), or pipe it from `crdgen` (best if changing it):

```sh
cargo run --bin crdgen | kubectl apply -f -
```

### Opentelemetry
Setup an opentelemetry collector in your cluster. [Tempo](https://github.com/grafana/helm-charts/tree/main/charts/tempo) / [opentelemetry-operator](https://github.com/open-telemetry/opentelemetry-helm-charts/tree/main/charts/opentelemetry-operator) / [grafana agent](https://github.com/grafana/helm-charts/tree/main/charts/agent-operator) should all work out of the box. If your collector does not support grpc otlp you need to change the exporter in [`main.rs`](./src/main.rs).

If you don't have a collector, you can build locally without the `telemetry` feature (`tilt up telemetry`), or pull images [without the `otel` tag](https://hub.docker.com/r/clux/controller/tags/).

## Running

### Locally

```sh
cargo run
```

or, with optional telemetry (change as per requirements):

```sh
OPENTELEMETRY_ENDPOINT_URL=https://0.0.0.0:55680 RUST_LOG=info,kube=trace,controller=debug cargo run --features=telemetry
```

### In-cluster
Use either your locally built image or the one from dockerhub (using opentelemetry features by default). Edit the [deployment](./yaml/deployment.yaml)'s image tag appropriately, and then:

```sh
kubectl apply -f yaml/deployment.yaml
kubectl wait --for=condition=available deploy/doc-controller --timeout=20s
kubectl port-forward service/doc-controller 8080:80
```

To build and deploy the image quickly, we recommend using [tilt](https://tilt.dev/), via `tilt up` instead.

## Usage
In either of the run scenarios, your app is listening on port `8080`, and it will observe `Document` events.

Try some of:

```sh
kubectl apply -f yaml/instance-lorem.yaml
kubectl delete doc lorem
kubectl edit doc lorem # change hidden
```

The reconciler will run and write the status object on every change. You should see results in the logs of the pod, or on the `.status` object outputs of `kubectl get doc -oyaml`.

### Webapp output
The sample web server exposes some example metrics and debug information you can inspect with `curl`.

```sh
$ kubectl apply -f yaml/instance-lorem.yaml
$ curl 0.0.0.0:8080/metrics
# HELP doc_controller_reconcile_duration_seconds The duration of reconcile to complete in seconds
# TYPE doc_controller_reconcile_duration_seconds histogram
doc_controller_reconcile_duration_seconds_bucket{le="0.01"} 1
doc_controller_reconcile_duration_seconds_bucket{le="0.1"} 1
doc_controller_reconcile_duration_seconds_bucket{le="0.25"} 1
doc_controller_reconcile_duration_seconds_bucket{le="0.5"} 1
doc_controller_reconcile_duration_seconds_bucket{le="1"} 1
doc_controller_reconcile_duration_seconds_bucket{le="5"} 1
doc_controller_reconcile_duration_seconds_bucket{le="15"} 1
doc_controller_reconcile_duration_seconds_bucket{le="60"} 1
doc_controller_reconcile_duration_seconds_bucket{le="+Inf"} 1
doc_controller_reconcile_duration_seconds_sum 0.013
doc_controller_reconcile_duration_seconds_count 1
# HELP doc_controller_reconciliation_errors_total reconciliation errors
# TYPE doc_controller_reconciliation_errors_total counter
doc_controller_reconciliation_errors_total 0
# HELP doc_controller_reconciliations_total reconciliations
# TYPE doc_controller_reconciliations_total counter
doc_controller_reconciliations_total 1
$ curl 0.0.0.0:8080/
{"last_event":"2019-07-17T22:31:37.591320068Z"}
```

The metrics will be scraped by prometheus if you setup a `PodMonitor` or `ServiceMonitor` for it.

### Events
The example `reconciler` only checks the `.spec.hidden` bool. If it does, it updates the `.status` object to reflect whether or not the instance `is_hidden`. It also sends a Kubernetes event associated with the controller. It is visible at the bottom of `kubectl describe doc samuel`.

To extend this controller for a real-world setting. Consider looking at the [kube.rs controller guide](https://kube.rs/controllers/intro/).
