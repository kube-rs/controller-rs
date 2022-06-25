use crate::{telemetry, Error, Result};
use chrono::{DateTime, Utc};
use futures::{future::BoxFuture, FutureExt, StreamExt};
use kube::{
    api::{Api, ListParams, Patch, PatchParams, ResourceExt},
    client::Client,
    runtime::{
        controller::{Action, Controller},
        events::{Event, EventType, Recorder, Reporter},
        finalizer,
        finalizer::Event as FinalizerEvent,
    },
    CustomResource, Resource,
};
use prometheus::{
    default_registry, proto::MetricFamily, register_histogram_vec, register_int_counter, HistogramVec,
    IntCounter,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::{
    sync::RwLock,
    time::{Duration, Instant},
};
use tracing::*;

static DOCUMENT_FINALIZER: &'static str = "finalizer.document.io";

/// Our document properties that will be wrapped in a Kubernetes resource as a Spec struct
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[kube(kind = "Document", group = "kube.rs", version = "v1", namespaced)]
#[kube(status = "DocumentStatus", shortname = "doc")]
pub struct DocumentSpec {
    title: String,
    hide: bool,
    content: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, JsonSchema)]
pub struct DocumentStatus {
    hidden: bool,
}

impl Document {
    fn was_hidden(&self) -> bool {
        self.status.as_ref().map(|s| s.hidden).unwrap_or(false)
    }
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

#[instrument(skip(ctx, doc), fields(trace_id))]
async fn reconcile(doc: Arc<Document>, ctx: Arc<Data>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let start = Instant::now();
    ctx.metrics.reconciliations.inc();
    let client = ctx.client.clone();
    let name = doc.name();
    let ns = doc.namespace().unwrap();
    let docs: Api<Document> = Api::namespaced(client, &ns);

    let action = finalizer(&docs, DOCUMENT_FINALIZER, doc, |event| async {
        match event {
            FinalizerEvent::Apply(doc) => do_reconcile(&docs, doc, ctx.clone()).await,
            FinalizerEvent::Cleanup(doc) => clean_up(doc, ctx.clone()).await,
        }
    })
    .await
    .map_err(Error::KubeError);

    let duration = start.elapsed().as_millis() as f64 / 1000.0;
    //let ex = Exemplar::new_with_labels(duration, HashMap::from([("trace_id".to_string(), trace_id)]);
    ctx.metrics
        .reconcile_duration
        .with_label_values(&[])
        .observe(duration);
    //.observe_with_exemplar(duration, ex);
    info!("Reconciled Document \"{}\" in {}", name, ns);

    action
}

async fn do_reconcile(
    docs: &Api<Document>,
    doc: Arc<Document>,
    ctx: Arc<Data>,
) -> Result<Action, kube::Error> {
    let client = ctx.client.clone();
    ctx.state.write().await.last_event = Utc::now();
    let reporter = ctx.state.read().await.reporter.clone();
    let recorder = Recorder::new(client.clone(), reporter, doc.object_ref(&()));
    let name = doc.name();

    let should_hide = doc.spec.hide;
    if doc.was_hidden() && should_hide {
        // only send event the first time
        recorder
            .publish(Event {
                type_: EventType::Normal,
                reason: "HiddenDoc".into(),
                note: Some(format!("Hiding `{}`", name)),
                action: "Reconciling".into(),
                secondary: None,
            })
            .await?;
    }
    // always overwrite status object with what we saw
    let new_status = Patch::Apply(json!({
        "apiVersion": "kube.rs/v1",
        "kind": "Document",
        "status": DocumentStatus {
            hidden: should_hide,
        }
    }));
    let ps = PatchParams::apply("cntrlr").force();
    let _o = docs.patch_status(&name, &ps, &new_status).await?;

    // If no events were received, check back every 30 minutes
    Ok(Action::requeue(Duration::from_secs(30 * 60)))
}

fn error_policy(error: &Error, ctx: Arc<Data>) -> Action {
    warn!("reconcile failed: {:?}", error);
    ctx.metrics.failures.inc();
    Action::requeue(Duration::from_secs(5 * 60))
}

async fn clean_up(doc: Arc<Document>, ctx: Arc<Data>) -> Result<Action, kube::Error> {
    let client = ctx.client.clone();
    ctx.state.write().await.last_event = Utc::now();
    let reporter = ctx.state.read().await.reporter.clone();
    let recorder = Recorder::new(client.clone(), reporter, doc.object_ref(&()));

    recorder
        .publish(Event {
            type_: EventType::Normal,
            reason: "DeleteDoc".into(),
            note: Some(format!("Delete `{}`", doc.name())),
            action: "Reconciling".into(),
            secondary: None,
        })
        .await?;

    Ok(Action::await_change())
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
            "doc_controller_reconcile_duration_seconds",
            "The duration of reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        )
        .unwrap();

        Metrics {
            reconciliations: register_int_counter!("doc_controller_reconciliations_total", "reconciliations")
                .unwrap(),
            failures: register_int_counter!(
                "doc_controller_reconciliation_errors_total",
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
            reporter: "doc-controller".into(),
        }
    }
}

/// Data owned by the Manager
#[derive(Clone)]
pub struct Manager {
    /// In memory state
    state: Arc<RwLock<State>>,
}

/// Example Manager that owns a Controller for Document
impl Manager {
    /// Lifecycle initialization interface for app
    ///
    /// This returns a `Manager` that drives a `Controller` + a future to be awaited
    /// It is up to `main` to wait for the controller stream.
    pub async fn new() -> (Self, BoxFuture<'static, ()>) {
        let client = Client::try_default().await.expect("create client");
        let metrics = Metrics::new();
        let state = Arc::new(RwLock::new(State::new()));
        let context = Arc::new(Data {
            client: client.clone(),
            metrics: metrics.clone(),
            state: state.clone(),
        });

        let docs = Api::<Document>::all(client);
        // Ensure CRD is installed before loop-watching
        let _r = docs
            .list(&ListParams::default().limit(1))
            .await
            .expect("is the crd installed? please run: cargo run --bin crdgen | kubectl apply -f -");

        // All good. Start controller and return its future.
        let drainer = Controller::new(docs, ListParams::default())
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
