use crate::{Document, Error};
use kube::ResourceExt;
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{
        counter::{Atomic, Counter},
        family::Family,
        histogram::Histogram,
    },
    registry::Registry,
};
use tokio::time::Instant;

#[derive(Clone)]
pub struct Metrics {
    pub reconciliations: Family<(), Counter>,
    pub failures: Family<ErrorLabels, Counter>,
    pub reconcile_duration: Histogram,
}

impl Default for Metrics {
    fn default() -> Self {
        let reconcile_duration = Histogram::new([0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.].into_iter());
        let failures = Family::<ErrorLabels, Counter>::default();
        let reconciliations = Family::<(), Counter>::default();
        Metrics {
            reconciliations,
            failures,
            reconcile_duration,
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct ErrorLabels {
    instance: String,
    error: String,
}

impl Metrics {
    /// Register API metrics to start tracking them.
    pub fn register(self, registry: &Registry) -> Self {
        registry.register(
            "doc_controller_reconcile_duration_seconds",
            "The duration of reconcile to complete in seconds",
            self.reconcile_duration.clone(),
        );
        registry.register(
            "doc_controller_reconciliation_errors_total",
            "reconciliation errors",
            self.failures.clone(),
        );
        registry.register(
            "doc_controller_reconciliations_total",
            "reconciliations",
            self.reconciliations.clone(),
        );
        self
    }

    pub fn reconcile_failure(&self, doc: &Document, e: &Error) {
        self.failures
            .get_or_create(&ErrorLabels {
                instance: doc.name_any(),
                error: e.metric_label(),
            })
            .inc();
    }

    pub fn count_and_measure(&self) -> ReconcileMeasurer {
        self.reconciliations.get_or_create(&()).inc();
        ReconcileMeasurer {
            start: Instant::now(),
            metric: self.reconcile_duration.clone(),
        }
    }
}

/// Smart function duration measurer
///
/// Relies on Drop to calculate duration and register the observation in the histogram
pub struct ReconcileMeasurer {
    start: Instant,
    metric: Histogram,
}

impl Drop for ReconcileMeasurer {
    fn drop(&mut self) {
        #[allow(clippy::cast_precision_loss)]
        let duration = self.start.elapsed().as_millis() as f64 / 1000.0;
        self.metric.observe(duration);
    }
}
