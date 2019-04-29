## operator-rs
[![CircleCI](https://circleci.com/gh/clux/operator-rs/tree/master.svg?style=shield)](https://circleci.com/gh/clux/operator-rs/tree/master)
[![docker pulls](https://img.shields.io/docker/pulls/clux/operator.svg)](
https://hub.docker.com/r/clux/operator/)
[![docker image info](https://images.microbadger.com/badges/image/clux/operator.svg)](http://microbadger.com/images/clux/operator)
[![docker tag](https://images.microbadger.com/badges/version/clux/operator-rs.svg)](https://hub.docker.com/r/clux/operator-rs/tags/)

A kubernetes operator for a `Foo` resource using reflectors in rust.

## Requirements
A kube cluster with access to read crds:

```sh
export NAMESPACE=kube-system # or edit yaml
kubectl apply -f yaml/access.yaml
```

Some sample custom resources installed in cluster:

```sh
kubectl apply -f yaml/examplecrd.yaml
kubectl apply -f yaml/crd-qux.yaml
```

Then with a valid kube config with sufficient access (`foobar` service account has sufficient acces), you can start the server with `cargo run`.

## Usage
Run with `cargo run` and inspect the state with `curl`:

```sh
$ cargo run # keep this running
$ curl localhost:8080/
{"qux":{"name":"baz","info":"this is baz"}}
```

Then you can try to remove a `foo`:

```sh
kubectl delete foo qux
```

and watch that the reflector picks up on in:

```
[2019-04-28T22:03:08Z INFO  operator::kube] Removing qux from foos
```

ditto if you try to apply one:

```sh
kubectl apply -f yaml/crd-baz.yaml
```

```
[2019-04-28T22:07:01Z INFO  operator::kube] Adding baz to foos
```

If you edit, and then apply, baz, you'll get:

```
[2019-04-28T22:08:21Z INFO  operator::kube] Modifying baz in foos
```

In all cases, the reflector maintains an internal state for the `Foo` custom resource, which you can verify with `curl`.
