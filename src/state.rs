use crate::{Error, FooPatchFailed, Result, SerializationFailed};
use chrono::prelude::*;
use futures::{future::BoxFuture, FutureExt, StreamExt};
use kube::{
    api::{Api, ListParams, Meta, PatchParams},
    client::Client,
};
use kube_derive::CustomResource;
use kube_runtime::{
    controller::{Context, Controller, ReconcilerAction},
    reflector::ObjectRef,
};
use prometheus::{default_registry, proto::MetricFamily, IntCounter, IntCounterVec, IntGauge, IntGaugeVec};
use serde_json::json;
use snafu::{Backtrace, OptionExt, ResultExt, Snafu};
use std::{
    collections::BTreeMap,
    env,
    sync::{Arc, Mutex},
};
use tokio::{sync::RwLock, time::Duration};

/// Our Foo custom resource spec
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug)]
#[kube(group = "clux.dev", version = "v1", namespaced)]
#[kube(apiextensions = "v1beta1")]
#[kube(status = "FooStatus")]
pub struct FooSpec {
    name: String,
    info: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct FooStatus {
    is_bad: bool,
    replicas: i32,
}

// Context for our reconciler
#[derive(Clone)]
struct Data {
    /// kubernetes client
    client: Client,
    /// In memory state
    state: Arc<RwLock<State>>,
    /// Various prometheus metrics
    metrics: Arc<RwLock<Metrics>>,
}

async fn reconcile(foo: Foo, ctx: Context<Data>) -> Result<ReconcilerAction, Error> {
    let client = ctx.get_ref().client.clone();
    ctx.get_ref().state.write().await.last_event = Utc::now();
    let name = Meta::name(&foo);
    let ns = Meta::namespace(&foo).expect("foo is namespaced");
    info!("Reconcile Foo {}: {:?}", name, foo);
    let foos: Api<Foo> = Api::namespaced(client, &ns);

    let new_status = serde_json::to_vec(&json!({
        "status": FooStatus {
            is_bad: foo.spec.info.contains("bad words"),
            replicas: 1
        }
    }))
    .context(SerializationFailed)?;
    let ss_apply = PatchParams::default_apply().force();
    let _o = foos
        .patch_status(&name, &ss_apply, new_status)
        .await
        .context(FooPatchFailed)?;

    // If no events were received, check back every 30 minutes
    Ok(ReconcilerAction {
        requeue_after: Some(Duration::from_secs(3600 / 2)),
    })
}
fn error_policy(error: &Error, _ctx: Context<Data>) -> ReconcilerAction {
    warn!("reconcile failed: {}", error);
    ReconcilerAction {
        requeue_after: Some(Duration::from_secs(360)),
    }
}

/// Metrics exposed on /metrics
#[derive(Clone)]
pub struct Metrics {
    pub handled_events: IntCounter,
}
impl Metrics {
    fn new() -> Self {
        Metrics {
            handled_events: register_int_counter!("handled_events", "handled events").unwrap(),
        }
    }
}

/// In-memory reconciler state exposed on /
#[derive(Clone, Serialize)]
pub struct State {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
}
impl State {
    fn new() -> Self {
        State {
            last_event: Utc::now(),
        }
    }
}

/// Data owned by the Manager
#[derive(Clone)]
pub struct Manager {
    /// In memory state
    state: Arc<RwLock<State>>,
    /// Various prometheus metrics
    metrics: Arc<RwLock<Metrics>>,
}

/// Example Manager that owns a Controller for Foo
impl Manager {
    /// Lifecycle initialization interface for app
    ///
    /// This returns a `Manager` that drives a `Controller` + a future to be awaited
    /// It is up to `main` to wait for the controller stream.
    pub fn new(client: Client) -> (Self, BoxFuture<'static, ()>) {
        let metrics = Arc::new(RwLock::new(Metrics::new()));
        let state = Arc::new(RwLock::new(State::new()));
        let context = Context::new(Data {
            client: client.clone(),
            metrics: metrics.clone(),
            state: state.clone(),
        });
        let foos = Api::<Foo>::all(client);

        let drainer = Controller::new(foos, ListParams::default())
            .run(reconcile, error_policy, context)
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|o| {
                println!("Reconciled {:?}", o);
                futures::future::ready(())
            })
            .boxed();
        // what we do with the controller stream (.run()) does not matter ^^
        // but we do need to consume it, hence general printing + return future

        (Self { state, metrics }, drainer)
    }

    /// Metrics getter
    pub fn metrics(&self) -> Vec<MetricFamily> {
        default_registry().gather()
    }

    /// State getter
    pub async fn state(&self) -> State {
        self.state.read().await.clone()
    }
}
