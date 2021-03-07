use crate::{Error, Result};
//use tracing_attributes::instrument;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{Registry, fmt::format::FmtSpan};
use tracing_subscriber::{fmt, EnvFilter};

pub fn init() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let otlp_endpoint = std::env::var("OPENTELEMETRY_ENDPOINT_URL")
        //.unwrap_or("http://grafana-agent-traces.monitoring.svc.cluster.local:55680")
        .unwrap_or("http://0.0.0.0:55680".to_string());

    let (tracer, _uninstall) = opentelemetry_otlp::new_pipeline()
        // NB: need to port-forward the service
        // k port-forward -n monitoring service/grafana-agent-traces 55680:55680
        .with_endpoint(&otlp_endpoint)
        .install()?;

    // Finish layers
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    let logger = tracing_subscriber::fmt::layer();//.with_span_events(FmtSpan::ACTIVE);

    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))?;

    // Register all subscribers
    let subscriber = Registry::default()
        .with(filter_layer)
        .with(telemetry)
        .with(logger);

    tracing::subscriber::set_global_default(subscriber)?;
    Ok(())
}

// attempt to get TraceId by span name via opentelemetry
// feels a bit like a backdoor, but it's not passed through all the layers
// ideally it would be exposed by tracing::Span
pub fn get_trace_id() -> String {
    use opentelemetry::trace::{SpanContext, TraceContextExt, Tracer};
    opentelemetry::global::tracer("registry").in_span("reconcile", |cx| {
        cx.span().span_context().trace_id().to_hex()
    })
}
