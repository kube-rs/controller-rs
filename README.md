## controller-rs
[![CircleCI](https://circleci.com/gh/kube-rs/controller-rs/tree/master.svg?style=shield)](https://circleci.com/gh/kube-rs/controller-rs/tree/master)
[![docker image](https://img.shields.io/docker/pulls/clux/controller.svg)](
https://hub.docker.com/r/clux/controller/tags/)

A rust kubernetes reference controller for a [`Foo` resource](https://github.com/kube-rs/controller-rs/blob/master/yaml/foo-crd.yaml) using [kube-rs](https://github.com/kube-rs/kube-rs/), with observability instrumentation.

The `Controller` object reconciles `Foo` instances when changes to it are detected, and writes to its .status object.

## Requirements
- A kube cluster / minikube / k3d.
- The CRD
- Opentelemetry collector (optional when building locally)

### CRD
Generate the CRD from the rust types and apply it to your cluster:

```sh
cargo run --bin crdgen > yaml/foo-crd.yaml
kubectl apply -f yaml/foo-crd.yaml
```

## Development Modes
You can either run locally, or build the image and deploy to your cluster.

### Local Development
You need a valid local kube config with rbac privilages described in the [deployment.yaml](./yaml/deployment.yaml). A default `k3d` setup will work.

This setup is the easiest (and fastest) since it can work with just `cargo run` (or port-forward + and set evar first for telemetry feature), but you won't have everything in the cluster at your disposal.

### In-cluster Development
Deploy as a deployment with scoped access via a service account. See `yaml/deployment.yaml` as an example. Note that the image on dockerhub is built with the `telemetry` feature (which requires an otel collector).

```sh
kubectl apply -f yaml/deployment.yaml
sleep 10 # wait for docker pull and start on kube side
export FOO_POD="$(kubectl get pods -n default -lapp=foo-controller --no-headers | awk '{print $1}')"
kubectl port-forward ${FOO_POD} -n default 8080:8080 &
```

This is the more complicated (and slightly slower) setup, since you need to configure the deployment to point at the otel colector, (and possibly build the image yourself to avoid telemetry). To simplify the above, we recommend using [tilt](https://tilt.dev/), via `tilt up` instead.

## Features
### Opentelemetry
When using the `telemetry` feature, you need an opentelemetry collector configured. Anything should work, but you might need to change the exporter in `main.rs` if it's not grpc otel.

If you have a running [Tempo](https://grafana.com/oss/tempo/) agent/deployment, you can `make forward-tempo` while developing locally, and configure the `OPENTELEMETRY_ENDPOINT_URL` evar as per `make run`.

You probably need to edit the `OPENTELEMETRY_ENDPOINT_URL` to fit your cluster.

## Usage
Once the app is running, you can see that it observes `foo` events.

Try some of:

```sh
kubectl apply -f yaml/instance-good.yaml -n default
kubectl delete foo good -n default
kubectl edit foo good # change info to contain bad
```

The reconciler will run and write the status object on every change. You should see results in the logs of the pod, or on the .status object outputs of `kubectl get foos -oyaml`.

## Webapp output
The sample web server exposes some example metrics and debug information you can inspect with `curl`.

```sh
$ kubectl apply -f yaml/instance-good.yaml -n default
$ curl 0.0.0.0:8080/metrics
# HELP handled_events handled events
# TYPE handled_events counter
handled_events 1
$ curl 0.0.0.0:8080/
{"last_event":"2019-07-17T22:31:37.591320068Z"}
```

The metrics will be auto-scraped if you have a standard [`PodMonitor` for `prometheus.io/scrape`](https://github.com/prometheus-community/helm-charts/blob/b69e89e73326e8b504102a75d668dc4351fcdb78/charts/prometheus/values.yaml#L1608-L1650).

## Events
The example `reconciler` only checks the `.spec.info` to see if it contains the word `bad`. If it does, it updates the `.status` object to reflect whether or not the instance `is_bad`.

While this controller has no child objects configured, there is a `configmapgen_controller` example in [kube-rs](https://github.com/kube-rs/kube-rs/).
