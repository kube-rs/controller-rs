//use opentelemetry::trace::TraceId;
use opentelemetry::global;
use opentelemetry::trace::{TraceContextExt, Tracer};
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::Config;
use opentelemetry_sdk::{runtime, trace as sdktrace, Resource};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{prelude::*, EnvFilter, Registry};

///  Fetch an opentelemetry::trace::TraceId as hex through the full tracing stack
pub fn get_trace_id() -> u64 {
    //use opentelemetry::trace::context::TraceContextExt as _; // opentelemetry::Context -> opentelemetry::trace::Span
    //use tracing_opentelemetry::OpenTelemetrySpanExt as _; // tracing::Span to opentelemetry::Context
    tracing::Span::current().id().unwrap().into_u64()
    /*tracing::Span::current()
    .context()
    .span()
    .span_context()
    .trace_id()*/
}

fn resource() -> Resource {
    Resource::new([
        KeyValue::new("service.name", env!("CARGO_PKG_NAME")),
        KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
    ])
}

#[cfg(feature = "telemetry")]
fn init_tracer() -> sdktrace::TracerProvider {
    let otlp_endpoint =
        std::env::var("OPENTELEMETRY_ENDPOINT_URL").expect("Need a otel tracing collector configured");

    /*let export_config = ExportConfig {
        endpoint: otlp_endpoint.clone(),
        ..ExportConfig::default()
    };*/
    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(otlp_endpoint),
        )
        .with_trace_config(Config::default().with_resource(resource()))
        .install_batch(runtime::Tokio)
        .expect("valid tracer")
}

/// Initialize tracing
pub async fn init() {
    // Setup tracing layers
    //#[cfg(feature = "telemetry")]
    let tracer = init_tracer();
    global::set_tracer_provider(tracer);

    //let telemetry = tracing_opentelemetry::layer().with_tracer(init_tracer());
    let logger = tracing_subscriber::fmt::layer().compact();
    let env_filter = EnvFilter::try_from_default_env()
        .or(EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        //.with(env_filter)
        .with(logger)
        .with(OpenTelemetryLayer::new(tracer))
        .init();

    // Decide on layers
    //#[cfg(feature = "telemetry")]
    //let collector = Registry::default().with(telemetry).with(logger).with(env_filter);
    //#[cfg(not(feature = "telemetry"))]
    //let collector = Registry::default().with(logger).with(env_filter);

    // Initialize tracing
    //tracing::subscriber::set_global_default(collector).unwrap();
}

#[cfg(test)]
mod test {
    // This test only works when telemetry is initialized fully
    // and requires OPENTELEMETRY_ENDPOINT_URL pointing to a valid server
    #[cfg(feature = "telemetry")]
    #[tokio::test]
    #[ignore = "requires a trace exporter"]
    async fn get_trace_id_returns_valid_traces() {
        use super::*;
        super::init().await;
        #[tracing::instrument(name = "test_span")] // need to be in an instrumented fn
        fn test_trace_id() -> u64 {
            get_trace_id()
        }
        assert_ne!(test_trace_id(), 0, "valid trace");
    }
}
