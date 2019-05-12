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

Start the server with `cargo run`, then inspect the state with `curl`:

```sh
export NAMESPACE=kube-system # specify if you applied it elsewhere
cargo run # keep this running
curl localhost:8080/
# {"qux":{"name":"baz","info":"this is baz"}}
```

### In-cluster Config
Deploy as a deployment with scoped access via a service account. See `yaml/deployment.yaml` as an example.

```sh
kubectl apply -f yaml/deployment.yaml
sleep 10 # wait for docker pull and start on kube side
export FOO_POD="$(kubectl get pods -n kube-system -lapp=foo-controller --no-headers | awk '{print $1}')"
kubectl port-forward ${FOO_POD} -n kube-system 8080:8080 # keep this running
curl localhost:8080/
# {"qux":{"name":"baz","info":"this is baz"}}
```

## Usage
Then you can try to remove a `foo`:

```sh
kubectl delete foo qux
```

and verify that the app handles the event:

```
[2019-04-28T22:03:08Z INFO  controller::state] Deleted Foo: qux
```

ditto if you try to apply one:

```sh
kubectl apply -f yaml/crd-baz.yaml
```

```
[2019-04-28T22:07:01Z INFO  controller::state] Adding Foo: baz (this is baz)
```

If you edit, and then apply, baz, you'll get:

```
[2019-04-28T22:08:21Z INFO  controller::state] Modifyied Foo: baz (edit str)
```

In all cases, the app maintains a simple state map for the `Foo` custom resource, which you can verify with `curl`.

## Events
The current `handle_event` fn only prints and builds up internal state at the moment, but you can perform arbitrary kube actions using the client. See [state.rs](https://github.com/clux/controller-rs/blob/master/src/state.rs)
