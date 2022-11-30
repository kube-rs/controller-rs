use crate::{Document, Error};
use kube::ResourceExt;
use prometheus::{
    register_histogram_vec, register_int_counter, register_int_counter_vec, HistogramVec, IntCounter,
    IntCounterVec,
};
use tokio::time::Instant;

#[derive(Clone)]
pub struct Metrics {
    pub reconciliations: IntCounter,
    pub failures: IntCounterVec,
    pub reconcile_duration: HistogramVec,
}

impl Metrics {
    pub fn default() -> Self {
        let reconcile_duration = register_histogram_vec!(
            "doc_controller_reconcile_duration_seconds",
            "The duration of reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        )
        .unwrap();
        let failures = register_int_counter_vec!(
            "doc_controller_reconciliation_errors_total",
            "reconciliation errors",
            &["instance", "error"]
        )
        .unwrap();
        let reconciliations =
            register_int_counter!("doc_controller_reconciliations_total", "reconciliations").unwrap();
        Metrics {
            reconciliations,
            failures,
            reconcile_duration,
        }
    }
}

impl Metrics {
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
