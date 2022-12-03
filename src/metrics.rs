use crate::{Document, Error};
use kube::ResourceExt;
use prometheus::{histogram_opts, opts, HistogramVec, IntCounter, IntCounterVec, Registry};
use tokio::time::Instant;

#[derive(Clone)]
pub struct Metrics {
    pub reconciliations: IntCounter,
    pub failures: IntCounterVec,
    pub reconcile_duration: HistogramVec,
}

impl Default for Metrics {
    fn default() -> Self {
        let reconcile_duration = HistogramVec::new(
            histogram_opts!(
                "doc_controller_reconcile_duration_seconds",
                "The duration of reconcile to complete in seconds"
            )
            .buckets(vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]),
            &[],
        )
        .unwrap();
        let failures = IntCounterVec::new(
            opts!(
                "doc_controller_reconciliation_errors_total",
                "reconciliation errors",
            ),
            &["instance", "error"],
        )
        .unwrap();
        let reconciliations =
            IntCounter::new("doc_controller_reconciliations_total", "reconciliations").unwrap();
        Metrics {
            reconciliations,
            failures,
            reconcile_duration,
        }
    }
}

impl Metrics {
    /// Register API metrics to start tracking them.
    pub fn register(self, registry: &Registry) -> Result<Self, prometheus::Error> {
        registry.register(Box::new(self.reconcile_duration.clone()))?;
        registry.register(Box::new(self.failures.clone()))?;
        registry.register(Box::new(self.reconciliations.clone()))?;
        Ok(self)
    }

    pub fn reconcile_failure(&self, doc: &Document, e: &Error) {
        self.failures
            .with_label_values(&[doc.name_any().as_ref(), e.metric_label().as_ref()])
            .inc()
    }

    pub fn count_and_measure(&self) -> ReconcileMeasurer {
        self.reconciliations.inc();
        ReconcileMeasurer {
            start: Instant::now(),
            metric: self.reconcile_duration.clone(),
        }
    }
}

pub struct ReconcileMeasurer {
    start: Instant,
    metric: HistogramVec,
}

impl Drop for ReconcileMeasurer {
    fn drop(&mut self) {
        #[allow(clippy::cast_precision_loss)]
        let duration = self.start.elapsed().as_millis() as f64 / 1000.0;
        self.metric.with_label_values(&[]).observe(duration);
    }
}

#[cfg(test)]
mod test {
    //use super::Metrics;
    use crate::{
        manager::{error_policy, reconcile, Context},
        Document,
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn new_documents_without_finalizers_gets_a_finalizer() {
        let (testctx, fakeserver, _) = Context::test();
        let doc = Document::test();
        // verify that doc gets a finalizer attached during reconcile
        fakeserver.handle_finalizer_creation(&doc);
        let res = reconcile(Arc::new(doc), testctx).await;
        assert!(res.is_ok(), "initial creation does not call our reconciler");
        // TODO: action should be await here
    }

    #[tokio::test]
    async fn test_document_sends_events_and_patches_doc() {
        let (testctx, fakeserver, _) = Context::test();
        let doc = Document::test().finalized();
        // verify that doc gets a finalizer attached during reconcile
        fakeserver.handle_event_publish_and_document_patch(&doc);
        let res = reconcile(Arc::new(doc), testctx).await;
        assert!(res.is_ok(), "initial creation does not call our reconciler");
        // TODO: action should be reconcile in N seconds here
    }

    #[tokio::test]
    async fn illegal_document_reconcile_errors_which_bumps_failure_metric() {
        let (testctx, fakeserver, _registry) = Context::test();
        let doc = Arc::new(Document::illegal().finalized());
        // verify that a finialized doc will run the apply part of the reconciler and publish an event
        fakeserver.handle_event_publish();
        let res = reconcile(doc.clone(), testctx.clone()).await;
        assert!(res.is_err(), "apply reconciler fails on illegal doc");
        let err = res.unwrap_err();
        assert!(err.to_string().contains("IllegalDocument"));
        // calling error policy with the reconciler error should cause the correct metric to be set
        error_policy(doc.clone(), &err, testctx.clone());
        //dbg!("actual metrics: {}", registry.gather());
        let failures = testctx
            .metrics
            .failures
            .with_label_values(&["illegal", "finalizererror(applyfailed(illegaldocument))"])
            .get();
        assert_eq!(failures, 1);
    }
}
