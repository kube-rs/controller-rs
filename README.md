## controller-rs
[![ci](https://github.com/kube-rs/controller-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/kube-rs/controller-rs/actions/workflows/ci.yml)

A rust kubernetes reference controller for a [`Document` resource](https://github.com/kube-rs/controller-rs/blob/main/yaml/crd.yaml) using [kube](https://github.com/kube-rs/kube/), with observability instrumentation.

The `Controller` object reconciles `Document` instances when changes to it are detected, writes to its `.status` object, creates associated events, and uses finalizers for guaranteed delete handling.

## Installation

### CRD
Apply the CRD from [cached file](yaml/crd.yaml), or pipe it from `crdgen` to pickup schema changes:

```sh
cargo run --bin crdgen | kubectl apply -f -
```

### Controller

Install the controller via `helm` by setting your preferred settings. For defaults:

```sh
helm template charts/doc-controller | kubectl apply -f -
kubectl wait --for=condition=available deploy/doc-controller --timeout=30s
kubectl port-forward service/doc-controller 8080:80
```

The helm chart sets up the [container](https://github.com/kube-rs/controller-rs/pkgs/container/controller) built from this repository.

### Opentelemetry

Build and run with `telemetry` feature, or configure it via `helm`:

```sh
helm template charts/doc-controller --set tracing.enabled=true | kubectl apply -f -
```

This requires an opentelemetry collector in your cluster. [Tempo](https://github.com/grafana/helm-charts/tree/main/charts/tempo) / [opentelemetry-operator](https://github.com/open-telemetry/opentelemetry-helm-charts/tree/main/charts/opentelemetry-operator) / [grafana agent](https://github.com/grafana/helm-charts/tree/main/charts/agent-operator) should all work out of the box. If your collector does not support grpc otlp you need to change the exporter in [`telemetry.rs`](./src/telemetry.rs).

Note that the [images are pushed either with or without the telemetry feature](https://hub.docker.com/r/clux/controller/tags/) depending on whether the tag includes `otel`.

### Metrics

Metrics is available on `/metrics` and a `ServiceMonitor` is configurable from the chart:

```sh
helm template charts/doc-controller --set serviceMonitor.enabled=true | kubectl apply -f -
```

## Running

### Locally

```sh
cargo run
```

or, with optional telemetry:

```sh
OPENTELEMETRY_ENDPOINT_URL=https://0.0.0.0:4317 RUST_LOG=info,kube=trace,controller=debug cargo run --features=telemetry
```

### In-cluster
For prebuilt, edit the [chart values](./charts/doc-controller/values.yaml) or [snapshotted yaml](./yaml/deployment.yaml) and apply as you see fit (like above).

To develop by building/reloading the deployment in k3d quickly, you can use [`tilt up`](https://tilt.dev/).

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

The metrics will be scraped by prometheus if you setup a`ServiceMonitor` for it.

### Events
The example `reconciler` only checks the `.spec.hidden` bool. If it does, it updates the `.status` object to reflect whether or not the instance `is_hidden`. It also sends a Kubernetes event associated with the controller. It is visible at the bottom of `kubectl describe doc samuel`.

To extend this controller for a real-world setting. Consider looking at the [kube.rs controller guide](https://kube.rs/controllers/intro/).
