use crate::{telemetry, Error, Result};
use chrono::prelude::*;
use futures::{future::BoxFuture, FutureExt, StreamExt};
use k8s_openapi::api::core::v1::ObjectReference;
use kube::{
    api::{Api, ListParams, Patch, PatchParams, ResourceExt},
    client::Client,
    runtime::{
        controller::{Action, Context, Controller},
        events::{Event, EventType, Recorder, Reporter},
    },
    CustomResource, Resource,
};
use prometheus::{
    default_registry, proto::MetricFamily, register_histogram_vec, register_int_counter, HistogramOpts,
    HistogramVec, IntCounter,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, sync::Arc};
use tokio::{
    sync::RwLock,
    time::{Duration, Instant},
};
use tracing::{debug, error, event, field, info, instrument, trace, warn, Level, Span};

/// Our Foo custom resource spec
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(kind = "Foo", group = "clux.dev", version = "v1", namespaced)]
#[kube(status = "FooStatus")]
pub struct FooSpec {
    name: String,
    info: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct FooStatus {
    is_bad: bool,
    //last_updated: Option<DateTime<Utc>>,
}

// Context for our reconciler
#[derive(Clone)]
struct Data {
    /// kubernetes client
    client: Client,
    /// In memory state
    state: Arc<RwLock<State>>,
    /// Various prometheus metrics
    metrics: Metrics,
}

#[instrument(skip(ctx), fields(trace_id))]
async fn reconcile(foo: Arc<Foo>, ctx: Context<Data>) -> Result<Action, Error> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let start = Instant::now();
    ctx.get_ref().metrics.reconciliations.inc();

    let client = ctx.get_ref().client.clone();
    ctx.get_ref().state.write().await.last_event = Utc::now();
    let reporter = ctx.get_ref().state.read().await.reporter.clone();
    let recorder = Recorder::new(client.clone(), reporter, foo.object_ref(&()));
    let name = ResourceExt::name(foo.as_ref());
    let ns = ResourceExt::namespace(foo.as_ref()).expect("foo is namespaced");
    let foos: Api<Foo> = Api::namespaced(client, &ns);

    let new_status = Patch::Apply(json!({
        "apiVersion": "clux.dev/v1",
        "kind": "Foo",
        "status": FooStatus {
            is_bad: foo.spec.info.contains("bad"),
            //last_updated: Some(Utc::now()),
        }
    }));
    let ps = PatchParams::apply("cntrlr").force();
    let _o = foos
        .patch_status(&name, &ps, &new_status)
        .await
        .map_err(Error::KubeError)?;

    if foo.spec.info.contains("bad") {
        recorder
            .publish(Event {
                type_: EventType::Normal,
                reason: "BadFoo".into(),
                note: Some(format!("Sending `{}` to detention", name)),
                action: "Correcting".into(),
                secondary: None,
            })
            .await
            .map_err(Error::KubeError)?;
    }

    let duration = start.elapsed().as_millis() as f64 / 1000.0;
    //let ex = Exemplar::new_with_labels(duration, HashMap::from([("trace_id".to_string(), trace_id)]);
    ctx.get_ref()
        .metrics
        .reconcile_duration
        .with_label_values(&[])
        .observe(duration);
    //.observe_with_exemplar(duration, ex);
    info!("Reconciled Foo \"{}\" in {}", name, ns);

    // If no events were received, check back every 30 minutes
    Ok(Action::requeue(Duration::from_secs(30 * 60)))
}

fn error_policy(error: &Error, ctx: Context<Data>) -> Action {
    warn!("reconcile failed: {:?}", error);
    ctx.get_ref().metrics.failures.inc();
    Action::requeue(Duration::from_secs(5 * 60))
}

/// Metrics exposed on /metrics
#[derive(Clone)]
pub struct Metrics {
    pub reconciliations: IntCounter,
    pub failures: IntCounter,
    pub reconcile_duration: HistogramVec,
}
impl Metrics {
    fn new() -> Self {
        let reconcile_histogram = register_histogram_vec!(
            "foo_controller_reconcile_duration_seconds",
            "The duration of reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        )
        .unwrap();

        Metrics {
            reconciliations: register_int_counter!("foo_controller_reconciliations_total", "reconciliations")
                .unwrap(),
            failures: register_int_counter!(
                "foo_controller_reconciliation_errors_total",
                "reconciliation errors"
            )
            .unwrap(),
            reconcile_duration: reconcile_histogram,
        }
    }
}

/// In-memory reconciler state exposed on /
#[derive(Clone, Serialize)]
pub struct State {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
    #[serde(skip)]
    pub reporter: Reporter,
}
impl State {
    fn new() -> Self {
        State {
            last_event: Utc::now(),
            reporter: "foo-controller".into(),
        }
    }
}

/// Data owned by the Manager
#[derive(Clone)]
pub struct Manager {
    /// In memory state
    state: Arc<RwLock<State>>,
}

/// Example Manager that owns a Controller for Foo
impl Manager {
    /// Lifecycle initialization interface for app
    ///
    /// This returns a `Manager` that drives a `Controller` + a future to be awaited
    /// It is up to `main` to wait for the controller stream.
    pub async fn new() -> (Self, BoxFuture<'static, ()>) {
        let client = Client::try_default().await.expect("create client");
        let metrics = Metrics::new();
        let state = Arc::new(RwLock::new(State::new()));
        let context = Context::new(Data {
            client: client.clone(),
            metrics: metrics.clone(),
            state: state.clone(),
        });

        let foos = Api::<Foo>::all(client);
        // Ensure CRD is installed before loop-watching
        let _r = foos
            .list(&ListParams::default().limit(1))
            .await
            .expect("is the crd installed? please run: cargo run --bin crdgen | kubectl apply -f -");

        // All good. Start controller and return its future.
        let drainer = Controller::new(foos, ListParams::default())
            .run(reconcile, error_policy, context)
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();

        (Self { state }, drainer)
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
