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
    #[metric(namespace = "doc_ctrl_reconcile")]
    pub reconcile: ReconcileMetrics,
}

/// Metrics related to the reconciler
#[derive(MetricGroup)]
#[metric(new())]
pub struct ReconcileMetrics {
    pub runs: Counter,
    pub failures: CounterVec<ErrorLabelSet>,
    #[metric(metadata = Thresholds::with_buckets([0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]), rename = "duration_seconds")]
    pub duration: Histogram<8>,
}

#[derive(LabelGroup)]
#[label(set = ErrorLabelSet)]
pub struct ErrorLabels<'a> {
    #[label(dynamic_with = lasso::ThreadedRodeo, default)]
    instance: &'a str,
    #[label(dynamic_with = lasso::ThreadedRodeo, default)]
    error: &'a str,
}

impl Default for ReconcileMetrics {
    fn default() -> Self {
        ReconcileMetrics::new()
    }
}

impl ReconcileMetrics {
    pub fn set_failure(&self, doc: &Document, e: &Error) {
        self.failures.inc(ErrorLabels {
            instance: doc.name_any().as_ref(),
            error: e.metric_label().as_ref(),
        })
    }

    pub fn count_and_measure(&self) -> HistogramTimer<'_, 8> {
        self.runs.inc();
        self.duration.start_timer()
    }

    #[cfg(test)]
    pub fn get_failures(&self, instance: &str, error: &str) -> u64 {
        let labels = ErrorLabels { instance, error };
        // awkward, but it gets the job done for tests
        let metric = self.failures.get_metric(self.failures.with_labels(labels));
        metric.count.load(std::sync::atomic::Ordering::Relaxed)
    }
}
