use crate::{Document, Error};
use kube::ResourceExt;
use opentelemetry::trace::TraceId;
use prometheus_client::{
    encoding::EncodeLabelSet,
    metrics::{counter::Counter, exemplar::HistogramWithExemplars, family::Family},
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

#[derive(Clone, Hash, PartialEq, Eq, EncodeLabelSet, Debug, Default)]
pub struct TraceLabel {
    pub trace_id: String,
}
impl TryFrom<&TraceId> for TraceLabel {
    type Error = anyhow::Error;

    fn try_from(id: &TraceId) -> Result<TraceLabel, Self::Error> {
        if std::matches!(id, &TraceId::INVALID) {
            anyhow::bail!("invalid trace id")
        } else {
            let trace_id = id.to_string();
            Ok(Self { trace_id })
        }
    }
}

#[derive(Clone)]
pub struct Reconciler {
    pub reconciliations: Family<(), Counter>,
    pub failures: Family<ErrorLabels, Counter>,
    pub reconcile_duration: HistogramWithExemplars<TraceLabel>,
}

impl Default for Reconciler {
    fn default() -> Self {
        Reconciler {
            reconciliations: Family::<(), Counter>::default(),
            failures: Family::<ErrorLabels, Counter>::default(),
            reconcile_duration: HistogramWithExemplars::new(
                [0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.].into_iter(),
            ),
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

    pub fn count_and_measure(&self, trace_id: &TraceId) -> ReconcileMeasurer {
        self.reconciliations.get_or_create(&()).inc();
        ReconcileMeasurer {
            start: Instant::now(),
            labels: trace_id.try_into().ok(),
            metric: self.reconcile_duration.clone(),
        }
    }
}

/// Smart function duration measurer
///
/// Relies on Drop to calculate duration and register the observation in the histogram
pub struct ReconcileMeasurer {
    start: Instant,
    labels: Option<TraceLabel>,
    metric: HistogramWithExemplars<TraceLabel>,
}

impl Drop for ReconcileMeasurer {
    fn drop(&mut self) {
        #[allow(clippy::cast_precision_loss)]
        let duration = self.start.elapsed().as_millis() as f64 / 1000.0;
        let labels = self.labels.take();
        self.metric.observe(duration, labels);
    }
}
