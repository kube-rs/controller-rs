//! Helper methods only available for tests
use crate::{Context, Document, DocumentSpec, DocumentStatus, Metrics, Result, DOCUMENT_FINALIZER};
use assert_json_diff::assert_json_include;
use http::{Request, Response};
use hyper::{body::to_bytes, Body};
use kube::{Client, Resource, ResourceExt};
use prometheus::Registry;
use std::sync::Arc;

impl Document {
    /// A document that will cause the reconciler to fail
    pub fn illegal() -> Self {
        let mut d = Document::new("illegal", DocumentSpec::default());
        d.meta_mut().namespace = Some("default".into());
        d
    }

    /// A normal test document
    pub fn test() -> Self {
        let mut d = Document::new("test", DocumentSpec::default());
        d.meta_mut().namespace = Some("default".into());
        d
    }

    /// Modify document to be set to hide
    pub fn needs_hide(mut self) -> Self {
        self.spec.hide = true;
        self
    }

    /// Modify document to set a deletion timestamp
    pub fn needs_delete(mut self) -> Self {
        use chrono::prelude::{DateTime, TimeZone, Utc};
        let now: DateTime<Utc> = Utc.with_ymd_and_hms(2017, 04, 02, 12, 50, 32).unwrap();
        use k8s_openapi::apimachinery::pkg::apis::meta::v1::Time;
        self.meta_mut().deletion_timestamp = Some(Time(now));
        self
    }

    /// Modify a document to have the expected finalizer
    pub fn finalized(mut self) -> Self {
        self.finalizers_mut().push(DOCUMENT_FINALIZER.to_string());
        self
    }

    /// Modify a document to have an expected status
    pub fn with_status(mut self, status: DocumentStatus) -> Self {
        self.status = Some(status);
        self
    }
}

// We wrap tower_test::mock::Handle
type ApiServerHandle = tower_test::mock::Handle<Request<Body>, Response<Body>>;
pub struct ApiServerVerifier(ApiServerHandle);

/// Scenarios we test for in ApiServerVerifier
pub enum Scenario {
    /// objects without finalizers will get a finalizer applied (and not call the apply loop)
    FinalizerCreation(Document),
    /// objects that do not fail and do not cause publishes will only patch
    StatusPatch(Document),
    /// finalized objects with hide set causes both an event and then a hide patch
    EventPublishThenStatusPatch(String, Document),
    /// finalized objects "with errors" (i.e. the "illegal" object) will short circuit the apply loop
    RadioSilence,
    /// objects with a deletion timestamp will run the cleanup loop sending event and removing the finalizer
    Cleanup(String, Document),
}

pub async fn timeout_after_1s(handle: tokio::task::JoinHandle<()>) {
    tokio::time::timeout(std::time::Duration::from_secs(1), handle)
        .await
        .expect("timeout on mock apiserver")
        .expect("scenario succeeded")
}

impl ApiServerVerifier {
    /// Tests only get to run specific scenarios that has matching handlers
    ///
    /// This setup makes it easy to handle multiple requests by chaining handlers together.
    ///
    /// NB: If the controller is making more calls than we are handling in the scenario,
    /// you then typically see a `KubeError(Service(Closed(())))` from the reconciler.
    ///
    /// You should await the `JoinHandle` (with a timeout) from this function to ensure that the
    /// scenario runs to completion (i.e. all expected calls were responded to),
    /// using the timeout to catch missing api calls to Kubernetes.
    pub fn run(self, scenario: Scenario) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            // moving self => one scenario per test
            match scenario {
                Scenario::FinalizerCreation(doc) => self.handle_finalizer_creation(doc).await,
                Scenario::StatusPatch(doc) => self.handle_status_patch(doc).await,
                Scenario::EventPublishThenStatusPatch(reason, doc) => {
                    self.handle_event_create(reason)
                        .await
                        .unwrap()
                        .handle_status_patch(doc)
                        .await
                }
                Scenario::RadioSilence => Ok(self),
                Scenario::Cleanup(reason, doc) => {
                    self.handle_event_create(reason)
                        .await
                        .unwrap()
                        .handle_finalizer_removal(doc)
                        .await
                }
            }
            .expect("scenario completed without errors");
        })
    }

    // chainable scenario handlers

    async fn handle_finalizer_creation(mut self, doc: Document) -> Result<Self> {
        let (request, send) = self.0.next_request().await.expect("service not called");
        // We expect a json patch to the specified document adding our finalizer
        assert_eq!(request.method(), http::Method::PATCH);
        assert_eq!(
            request.uri().to_string(),
            format!(
                "/apis/kube.rs/v1/namespaces/default/documents/{}?",
                doc.name_any()
            )
        );
        let expected_patch = serde_json::json!([
            { "op": "test", "path": "/metadata/finalizers", "value": null },
            { "op": "add", "path": "/metadata/finalizers", "value": vec![DOCUMENT_FINALIZER] }
        ]);
        let req_body = to_bytes(request.into_body()).await.unwrap();
        let runtime_patch: serde_json::Value =
            serde_json::from_slice(&req_body).expect("valid document from runtime");
        assert_json_include!(actual: runtime_patch, expected: expected_patch);

        let response = serde_json::to_vec(&doc.finalized()).unwrap(); // respond as the apiserver would have
        send.send_response(Response::builder().body(Body::from(response)).unwrap());
        Ok(self)
    }

    async fn handle_finalizer_removal(mut self, doc: Document) -> Result<Self> {
        let (request, send) = self.0.next_request().await.expect("service not called");
        // We expect a json patch to the specified document removing our finalizer (at index 0)
        assert_eq!(request.method(), http::Method::PATCH);
        assert_eq!(
            request.uri().to_string(),
            format!(
                "/apis/kube.rs/v1/namespaces/default/documents/{}?",
                doc.name_any()
            )
        );
        let expected_patch = serde_json::json!([
            { "op": "test", "path": "/metadata/finalizers/0", "value": DOCUMENT_FINALIZER },
            { "op": "remove", "path": "/metadata/finalizers/0", "path": "/metadata/finalizers/0" }
        ]);
        let req_body = to_bytes(request.into_body()).await.unwrap();
        let runtime_patch: serde_json::Value =
            serde_json::from_slice(&req_body).expect("valid document from runtime");
        assert_json_include!(actual: runtime_patch, expected: expected_patch);

        let response = serde_json::to_vec(&doc).unwrap(); // respond as the apiserver would have
        send.send_response(Response::builder().body(Body::from(response)).unwrap());
        Ok(self)
    }

    async fn handle_event_create(mut self, reason: String) -> Result<Self> {
        let (request, send) = self.0.next_request().await.expect("service not called");
        assert_eq!(request.method(), http::Method::POST);
        assert_eq!(
            request.uri().to_string(),
            format!("/apis/events.k8s.io/v1/namespaces/default/events?")
        );
        // verify the event reason matches the expected
        let req_body = to_bytes(request.into_body()).await.unwrap();
        let postdata: serde_json::Value =
            serde_json::from_slice(&req_body).expect("valid event from runtime");
        dbg!("postdata for event: {}", postdata.clone());
        assert_eq!(
            postdata.get("reason").unwrap().as_str().map(String::from),
            Some(reason)
        );
        // then pass through the body
        send.send_response(Response::builder().body(Body::from(req_body)).unwrap());
        Ok(self)
    }

    async fn handle_status_patch(mut self, doc: Document) -> Result<Self> {
        let (request, send) = self.0.next_request().await.expect("service not called");
        assert_eq!(request.method(), http::Method::PATCH);
        assert_eq!(
            request.uri().to_string(),
            format!(
                "/apis/kube.rs/v1/namespaces/default/documents/{}/status?&force=true&fieldManager=cntrlr",
                doc.name_any()
            )
        );
        let req_body = to_bytes(request.into_body()).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&req_body).expect("patch_status object is json");
        let status_json = json.get("status").expect("status object").clone();
        let status: DocumentStatus = serde_json::from_value(status_json).expect("valid status");
        assert_eq!(status.hidden, doc.spec.hide, "status.hidden iff doc.spec.hide");
        let response = serde_json::to_vec(&doc.with_status(status)).unwrap();
        // pass through document "patch accepted"
        send.send_response(Response::builder().body(Body::from(response)).unwrap());
        Ok(self)
    }
}


impl Context {
    // Create a test context with a mocked kube client, locally registered metrics and default diagnostics
    pub fn test() -> (Arc<Self>, ApiServerVerifier, Registry) {
        let (mock_service, handle) = tower_test::mock::pair::<Request<Body>, Response<Body>>();
        let mock_client = Client::new(mock_service, "default");
        let registry = Registry::default();
        let ctx = Self {
            client: mock_client,
            metrics: Metrics::default().register(&registry).unwrap(),
            diagnostics: Arc::default(),
        };
        (Arc::new(ctx), ApiServerVerifier(handle), registry)
    }
}
