use crate::{Document, Error};
use kube::ResourceExt;
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{counter::Counter, family::Family, histogram::Histogram},
    registry::{Registry, Unit},
};
use std::sync::Arc;
use tokio::time::Instant;

#[derive(Clone)]
pub struct Metrics {
    pub reconciler: Reconciler,
    pub registry: Arc<Registry>,
}

impl Default for Metrics {
    fn default() -> Self {
        let mut registry = Registry::with_prefix("doc_ctrl");
        let reconciler = Reconciler::default().register(&mut registry);
        Self {
            registry: Arc::new(registry),
            reconciler,
        }
    }
}

#[derive(Clone)]
pub struct Reconciler {
    pub reconciliations: Family<(), Counter>,
    pub failures: Family<ErrorLabels, Counter>,
    pub reconcile_duration: Histogram,
}

impl Default for Reconciler {
    fn default() -> Self {
        Reconciler {
            reconciliations: Family::<(), Counter>::default(),
            failures: Family::<ErrorLabels, Counter>::default(),
            reconcile_duration: Histogram::new([0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.].into_iter()),
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct ErrorLabels {
    pub instance: String,
    pub error: String,
}

impl Reconciler {
    /// Register API metrics to start tracking them.
    pub fn register(self, registry: &mut Registry) -> Self {
        registry.register_with_unit(
            "reconcile_duration_seconds",
            "The duration of reconcile to complete in seconds",
            Unit::Seconds,
            self.reconcile_duration.clone(),
        );
        registry.register(
            "reconciliation_errors_total",
            "reconciliation errors",
            self.failures.clone(),
        );
        registry.register(
            "reconciliations_total",
            "reconciliations",
            self.reconciliations.clone(),
        );
        self
    }

    pub fn set_failure(&self, doc: &Document, e: &Error) {
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
