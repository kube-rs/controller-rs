use crate::{Document, Error};
use kube::ResourceExt;
use measured::{
    metric::histogram::{HistogramTimer, Thresholds},
    text::BufferedTextEncoder,
    Counter, CounterVec, Histogram, LabelGroup, MetricGroup,
};
use tokio::sync::Mutex;

/// Metrics with handler
#[derive(Default)]
pub struct Metrics {
    pub encoder: Mutex<BufferedTextEncoder>,
    pub app: AppMetrics,
}

/// All metrics
#[derive(MetricGroup, Default)]
pub struct AppMetrics {
    #[metric(namespace = "doc_ctrl")]
    pub reconciler: ReconcilerMetrics,
}

/// Metrics related to the reconciler
#[derive(MetricGroup)]
#[metric(new())]
pub struct ReconcilerMetrics {
    /// reconciliations
    pub reconciliations: Counter,
    /// reconciliation errors
    pub failures: CounterVec<ErrorLabelSet>,
    /// duration of reconcile to complete in seconds
    #[metric(metadata = Thresholds::with_buckets([0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]))]
    pub reconcile_duration: Histogram<8>,
}

#[derive(LabelGroup)]
#[label(set = ErrorLabelSet)]
pub struct ErrorLabels<'a> {
    #[label(dynamic_with = lasso::ThreadedRodeo, default)]
    instance: &'a str,
    #[label(dynamic_with = lasso::ThreadedRodeo, default)]
    error: &'a str,
}

impl Default for ReconcilerMetrics {
    fn default() -> Self {
        ReconcilerMetrics::new()
    }
}

impl ReconcilerMetrics {
    pub fn set_failure(&self, doc: &Document, e: &Error) {
        self.failures.inc(ErrorLabels {
            instance: doc.name_any().as_ref(),
            error: e.metric_label().as_ref(),
        })
    }

    pub fn count_and_measure(&self) -> HistogramTimer<'_, 8> {
        self.reconciliations.inc();
        self.reconcile_duration.start_timer()
    }

    #[cfg(test)]
    pub fn get_failures(&self, instance: &str, error: &str) -> u64 {
        let labels = ErrorLabels { instance, error };
        // awkward, but it gets the job done for tests
        let metric = self.failures.get_metric(self.failures.with_labels(labels));
        metric.count.load(std::sync::atomic::Ordering::Relaxed)
    }
}
