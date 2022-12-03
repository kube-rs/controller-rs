use crate::{telemetry, Error, Metrics, Result};
use chrono::{DateTime, Utc};
use futures::{future::BoxFuture, FutureExt, StreamExt};
use kube::{
    api::{Api, ListParams, Patch, PatchParams, ResourceExt},
    client::Client,
    runtime::{
        controller::{Action, Controller},
        events::{Event, EventType, Recorder, Reporter},
        finalizer::{finalizer, Event as Finalizer},
    },
    CustomResource, Resource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::{sync::RwLock, time::Duration};
use tracing::*;

pub static DOCUMENT_FINALIZER: &str = "documents.kube.rs";

/// Generate the Kubernetes wrapper struct `Document` from our Spec and Status struct
///
/// This provides a hook for generating the CRD yaml (in crdgen.rs)
#[derive(CustomResource, Deserialize, Serialize, Clone, Debug, JsonSchema)]
#[cfg_attr(test, derive(Default))]
#[kube(kind = "Document", group = "kube.rs", version = "v1", namespaced)]
#[kube(status = "DocumentStatus", shortname = "doc")]
pub struct DocumentSpec {
    pub title: String,
    pub hide: bool,
    pub content: String,
}
/// The status object of `Document`
#[derive(Deserialize, Serialize, Clone, Default, Debug, JsonSchema)]
pub struct DocumentStatus {
    pub hidden: bool,
}

impl Document {
    fn was_hidden(&self) -> bool {
        self.status.as_ref().map(|s| s.hidden).unwrap_or(false)
    }
}

// Context for our reconciler
#[derive(Clone)]
pub struct Context {
    /// Kubernetes client
    pub client: Client,
    /// Diagnostics read by the web server
    pub diagnostics: Arc<RwLock<Diagnostics>>,
    /// Prometheus metrics
    pub metrics: Metrics,
}

#[instrument(skip(ctx, doc), fields(trace_id))]
pub async fn reconcile(doc: Arc<Document>, ctx: Arc<Context>) -> Result<Action> {
    let trace_id = telemetry::get_trace_id();
    Span::current().record("trace_id", &field::display(&trace_id));
    let _timer = ctx.metrics.count_and_measure();
    ctx.diagnostics.write().await.last_event = Utc::now();
    let ns = doc.namespace().unwrap(); // doc is namespace scoped
    let docs: Api<Document> = Api::namespaced(ctx.client.clone(), &ns);

    info!("Reconciling Document \"{}\" in {}", doc.name_any(), ns);
    finalizer(&docs, DOCUMENT_FINALIZER, doc, |event| async {
        match event {
            Finalizer::Apply(doc) => doc.reconcile(ctx.clone()).await,
            Finalizer::Cleanup(doc) => doc.cleanup(ctx.clone()).await,
        }
    })
    .await
    .map_err(|e| Error::FinalizerError(Box::new(e)))
}

pub fn error_policy(doc: Arc<Document>, error: &Error, ctx: Arc<Context>) -> Action {
    warn!("reconcile failed: {:?}", error);
    ctx.metrics.reconcile_failure(&doc, error);
    Action::requeue(Duration::from_secs(5 * 60))
}

impl Document {
    // Reconcile (for non-finalizer related changes)
    async fn reconcile(&self, ctx: Arc<Context>) -> Result<Action> {
        let client = ctx.client.clone();
        let recorder = ctx.diagnostics.read().await.recorder(client.clone(), &self);
        let ns = self.namespace().unwrap();
        let name = self.name_any();
        let docs: Api<Document> = Api::namespaced(client, &ns);

        let should_hide = self.spec.hide;
        if !self.was_hidden() && should_hide {
            // only send event the first time
            recorder
                .publish(Event {
                    type_: EventType::Normal,
                    reason: "HiddenDoc".into(),
                    note: Some(format!("Hiding `{}`", name)),
                    action: "Reconciling".into(),
                    secondary: None,
                })
                .await
                .map_err(Error::KubeError)?;
        }
        if name == "illegal" {
            return Err(Error::IllegalDocument); // error names show up in metrics
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
        let _o = docs
            .patch_status(&name, &ps, &new_status)
            .await
            .map_err(Error::KubeError)?;

        // If no events were received, check back every 5 minutes
        Ok(Action::requeue(Duration::from_secs(5 * 60)))
    }

    // Finalizer cleanup (the object was deleted, ensure nothing is orphaned)
    async fn cleanup(&self, ctx: Arc<Context>) -> Result<Action> {
        let recorder = ctx.diagnostics.read().await.recorder(ctx.client.clone(), &self);
        // Document doesn't have dependencies in this example case, so we just publish an event
        recorder
            .publish(Event {
                type_: EventType::Normal,
                reason: "DeleteDoc".into(),
                note: Some(format!("Delete `{}`", self.name_any())),
                action: "Reconciling".into(),
                secondary: None,
            })
            .await
            .map_err(Error::KubeError)?;
        Ok(Action::await_change())
    }
}

/// Diagnostics to be exposed by the web server
#[derive(Clone, Serialize)]
pub struct Diagnostics {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
    #[serde(skip)]
    pub reporter: Reporter,
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            last_event: Utc::now(),
            reporter: "doc-controller".into(),
        }
    }
}
impl Diagnostics {
    fn recorder(&self, client: Client, doc: &Document) -> Recorder {
        Recorder::new(client, self.reporter.clone(), doc.object_ref(&()))
    }
}

/// Data owned by the Manager
#[derive(Clone, Default)]
pub struct Manager {
    /// Diagnostics populated by the reconciler
    diagnostics: Arc<RwLock<Diagnostics>>,
    /// Metrics registry
    registry: prometheus::Registry,
}

/// Example Manager that owns a Controller for Document
impl Manager {
    /// Lifecycle initialization interface for app
    ///
    /// This returns a `Manager` that drives a `Controller` + a future to be awaited
    /// It is up to `main` to wait for the controller stream.
    pub async fn new() -> (Self, BoxFuture<'static, ()>) {
        let client = Client::try_default().await.expect("create client");
        let manager = Manager::default();
        let context = Arc::new(Context {
            client: client.clone(),
            metrics: Metrics::default().register(&manager.registry).unwrap(),
            diagnostics: manager.diagnostics.clone(),
        });

        let docs = Api::<Document>::all(client);
        // Ensure CRD is installed before loop-watching
        let _r = docs
            .list(&ListParams::default().limit(1))
            .await
            .expect("is the crd installed? please run: cargo run --bin crdgen | kubectl apply -f -");

        // All good. Start controller and return its future.
        let controller = Controller::new(docs, ListParams::default())
            .run(reconcile, error_policy, context)
            .filter_map(|x| async move { std::result::Result::ok(x) })
            .for_each(|_| futures::future::ready(()))
            .boxed();

        (manager, controller)
    }

    /// Metrics getter
    pub fn metrics(&self) -> Vec<prometheus::proto::MetricFamily> {
        self.registry.gather()
    }

    /// State getter
    pub async fn diagnostics(&self) -> Diagnostics {
        self.diagnostics.read().await.clone()
    }
}
