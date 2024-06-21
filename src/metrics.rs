use crate::{Document, Error};
use kube::ResourceExt;
use measured::{
    metric::histogram::{HistogramVecTimer, Thresholds},
    text::BufferedTextEncoder,
    CounterVec, HistogramVec, LabelGroup, MetricGroup,
};
use tokio::sync::Mutex;

#[derive(Default)]
pub struct Metrics {
    pub encoder: Mutex<BufferedTextEncoder>,
    pub reconciler: ReconcilerMetrics,
}

#[derive(MetricGroup)]
#[metric(new())]
pub struct ReconcilerMetrics {
    /// reconciliations
    #[metric(rename = "doc_controller_reconciliations_total")]
    #[metric(label_set = EmptyLabelSet::default())]
    pub reconciliations: CounterVec<EmptyLabelSet>,
    /// reconciliation errors
    #[metric(rename = "doc_controller_reconciliation_errors_total")]
    #[metric(label_set = ErrorLabelSet::new())]
    pub failures: CounterVec<ErrorLabelSet>,
    /// duration of reconcile to complete in seconds
    #[metric(rename = "doc_controller_reconcile_duration_seconds")]
    #[metric(metadata = Thresholds::with_buckets([0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]))]
    pub reconcile_duration: HistogramVec<EmptyLabelSet, 8>,
}

#[derive(LabelGroup)]
#[label(set = ErrorLabelSet)]
pub struct ErrorLabels<'a> {
    #[label(dynamic_with = lasso::ThreadedRodeo, default)]
    instance: &'a str,
    #[label(dynamic_with = lasso::ThreadedRodeo, default)]
    error: &'a str,
}

#[derive(LabelGroup, Default)]
#[label(set = EmptyLabelSet)]
pub struct EmptyLabels {}

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

    pub fn count_and_measure(&self) -> HistogramVecTimer<'_, EmptyLabelSet, 8> {
        self.reconciliations.inc(EmptyLabels::default());
        self.reconcile_duration.start_timer(EmptyLabels::default())
    }

    #[cfg(test)]
    pub fn get_failures(&self, instance: &str, error: &str) -> u64 {
        let labels = ErrorLabels { instance, error };
        // awkward, but it gets the job done for tests
        let metric = self.failures.get_metric(self.failures.with_labels(labels));
        metric.count.load(std::sync::atomic::Ordering::Relaxed)
    }
}
