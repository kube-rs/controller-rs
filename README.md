## controller-rs
[![CircleCI](https://circleci.com/gh/clux/controller-rs/tree/master.svg?style=shield)](https://circleci.com/gh/clux/controller-rs/tree/master)
[![docker pulls](https://img.shields.io/docker/pulls/clux/controller.svg)](
https://hub.docker.com/r/clux/controller/)
[![docker image info](https://images.microbadger.com/badges/image/clux/controller.svg)](http://microbadger.com/images/clux/controller)
[![docker tag](https://images.microbadger.com/badges/version/clux/controller.svg)](https://hub.docker.com/r/clux/controller/tags/)

A kubernetes controller for a `Foo` resource using informers in rust.

## Requirements
A kube cluster / minikube. Install the CRD and an instance of it into the cluster:

```sh
kubectl apply -f yaml/examplecrd.yaml
kubectl apply -f yaml/crd-qux.yaml
```

## Running

### Local Config
You need a valid local kube config with sufficient access (`foobar` service account has sufficient access if you want to [impersonate](https://clux.github.io/probes/post/2019-03-31-impersonating-kube-accounts/) the one in `yaml/access.yaml`).

Start the server with `cargo run`:

```sh
export NAMESPACE=default
cargo run
```

### In-cluster Config
Deploy as a deployment with scoped access via a service account. See `yaml/deployment.yaml` as an example.

```sh
kubectl apply -f yaml/deployment.yaml
sleep 10 # wait for docker pull and start on kube side
export FOO_POD="$(kubectl get pods -n default -lapp=foo-controller --no-headers | awk '{print $1}')"
kubectl port-forward ${FOO_POD} -n default 8080:8080 # keep this running
```

## Usage
Once the app is running, you can see that it observes `foo` events.

You can try to remove a `foo`:

```sh
kubectl delete foo qux -n default
```

then the app will soon print:

```
[2019-04-28T22:03:08Z INFO  controller::state] Deleted Foo: qux
```

ditto if you try to apply one:

```sh
kubectl apply -f yaml/crd-baz.yaml -n default
```

```
[2019-04-28T22:07:01Z INFO  controller::state] Adding Foo: baz (this is baz)
```

If you edit, and then apply, baz, you'll get:

```
[2019-04-28T22:08:21Z INFO  controller::state] Modifyied Foo: baz (edit str)
```

## Webapp output
The sample web server exposes some example metrics and debug information you can inspect with `curl`.

```sh
$ kubectl apply -f yaml/crd-qux.yaml -n default
$ curl localhost:8080/metrics
# HELP handled_events handled events
# TYPE handled_events counter
handled_events 1
$ curl localhost:8080/
{"last_event":"2019-07-17T22:31:37.591320068Z"}
```

## Events
The event handler in [controller.rs](https://github.com/clux/controller-rs/blob/master/src/state.rs) currently does not mutate anything in kubernetes based on any events here as this is an example.

You can perform arbitrary kube actions using the `client`. See [kube-rs/examples](https://github.com/clux/kube-rs/tree/master/examples) and the api docs for [kube::api::Api](https://clux.github.io/kube-rs/kube/api/struct.Api.html) for ideas.
