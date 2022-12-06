//! Helper methods only available for tests
use crate::{Context, Document, DocumentSpec, DocumentStatus, Metrics, DOCUMENT_FINALIZER};
use assert_json_diff::assert_json_include;
use futures::pin_mut;
use http::{Request, Response};
use hyper::{body::to_bytes, Body};
use kube::{Client, Resource, ResourceExt};
use prometheus::Registry;
use std::sync::Arc;
use tokio::task::JoinHandle;
use tower_test::mock::{self, Handle};

impl Document {
    /// A document that will cause the reconciler to fail
    pub fn illegal() -> Self {
        let mut d = Document::new("illegal", DocumentSpec::default());
        d.meta_mut().namespace = Some("testns".into());
        d
    }

    /// A normal test document
    pub fn test() -> Self {
        let mut d = Document::new("testdoc", DocumentSpec::default());
        d.meta_mut().namespace = Some("testns".into());
        d.spec.hide = true;
        d
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
pub struct ApiServerVerifier(Handle<Request<Body>, Response<Body>>);

/// Create a responder + verifier object that deals with the main reconcile scenarios
///
/// 1. objects without finalizers will get a finalizer applied (and not call the apply loop)
/// 2. finalized objects will run through the apply loop
/// 3. finalized objects "with errors" (i.e. the "illegal" object) will short circuit the apply loop
/// 4. objects with a deletion timestamp will run the cleanup loop (which will send an event)
impl ApiServerVerifier {
    pub fn handle_finalizer_creation(self, doc_: &Document) -> JoinHandle<()> {
        let handle = self.0;
        let doc = doc_.clone();
        tokio::spawn(async move {
            pin_mut!(handle);
            let (request, send) = handle.next_request().await.expect("service not called");
            // We expect a json patch to the specified document adding our finalizer
            assert_eq!(request.method(), http::Method::PATCH);
            assert_eq!(
                request.uri().to_string(),
                format!("/apis/kube.rs/v1/namespaces/testns/documents/{}?", doc.name_any())
            );
            let expected_patch = serde_json::json!([
                { "op": "test", "path": "/metadata/finalizers", "value": null },
                { "op": "add", "path": "/metadata/finalizers", "value": vec![DOCUMENT_FINALIZER] }
            ]);
            let req_body = to_bytes(request.into_body()).await.unwrap();
            let runtime_patch: serde_json::Value =
                serde_json::from_slice(&req_body).expect("valid document from runtime");
            assert_json_include!(actual: runtime_patch, expected: expected_patch);

            let response = serde_json::to_vec(&doc.finalized()); // respond as the apiserver would have
            send.send_response(Response::builder().body(Body::from(response)).unwrap());
        })
    }

    pub fn handle_event_publish(self) -> JoinHandle<()> {
        let handle = self.0;
        tokio::spawn(async move {
            pin_mut!(handle);
            let (request, send) = handle.next_request().await.expect("service not called");
            assert_eq!(request.method(), http::Method::POST);
            assert_eq!(
                request.uri().to_string(),
                format!("/apis/events.k8s.io/v1/namespaces/testns/events?")
            );
            // pass the event straight through
            send.send_response(Response::builder().body(request.into_body()).unwrap());
        })
    }

    pub fn handle_event_publish_and_document_patch(self, doc_: &Document) -> JoinHandle<()> {
        let handle = self.0;
        let doc = doc_.clone();
        tokio::spawn(async move {
            pin_mut!(handle);
            // first expected request (same as handle_event_publish)
            // TODO: find a nice way to re-use the handle between a single test (duplicating logic here atm)
            let (request, send) = handle.next_request().await.expect("service not called");
            assert_eq!(request.method(), http::Method::POST);
            assert_eq!(
                request.uri().to_string(),
                format!("/apis/events.k8s.io/v1/namespaces/testns/events?")
            );
            send.send_response(Response::builder().body(request.into_body()).unwrap());
            // second expected request
            let (request, send) = handle
                .next_request()
                .await
                .expect("service not called second time");
            assert_eq!(request.method(), http::Method::PATCH);
            assert_eq!(
                request.uri().to_string(),
                format!(
                    "/apis/kube.rs/v1/namespaces/testns/documents/{}/status?&force=true&fieldManager=cntrlr",
                    doc.name_any()
                )
            );
            let req_body = to_bytes(request.into_body()).await.unwrap();
            let json: serde_json::Value =
                serde_json::from_slice(&req_body).expect("patch_status object is json");
            let status_json = json.get("status").expect("status object").clone();
            let status: DocumentStatus = serde_json::from_value(status_json).expect("contains valid status");
            assert_eq!(
                status.hidden, true,
                "Document::test sets hide so reconciler wants to hide it"
            );

            let response = serde_json::to_vec(&doc.with_status(status)).unwrap();
            // pass through document "patch accepted"
            send.send_response(Response::builder().body(Body::from(&response)).unwrap());
        })
    }
}

impl Context {
    // Create a test context with a mocked kube client, unregistered metrics and default diagnostics
    pub fn test() -> (Arc<Self>, ApiServerVerifier, Registry) {
        let (mock_service, handle) = mock::pair::<Request<Body>, Response<Body>>();
        let mock_client = Client::new(mock_service, "default");
        let registry = Registry::default();
        (
            Arc::new(Self {
                client: mock_client,
                metrics: Metrics::default().register(&registry).unwrap(),
                diagnostics: Arc::default(),
            }),
            ApiServerVerifier(handle),
            registry,
        )
    }
}
