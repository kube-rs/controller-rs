use prometheus::{
    default_registry,
    proto::MetricFamily,
    {IntCounter, IntCounterVec, IntGauge, IntGaugeVec},
};
use kube::{
    client::APIClient,
    config::Configuration,
    api::{Informer, WatchEvent, Object, Api, Void},
};
use chrono::prelude::*;
use std::{
    env,
    collections::BTreeMap,
    sync::{Arc, RwLock},
};
use crate::*;

/// Approximation of the CRD we want to work with
/// Replace with own struct.
/// Add serialize for returnability.
#[derive(Deserialize, Serialize, Clone)]
pub struct FooSpec {
  name: String,
  info: String,
}
/// Type alias for the kubernetes object
type Foo = Object<FooSpec, Void>;

/// Metrics exposed on /metrics
#[derive(Clone)]
pub struct Metrics {
    pub handled_events: IntCounter,
}
impl Metrics {
    fn new() -> Self {
        Metrics {
            handled_events: register_int_counter!("handled_events", "handled events").unwrap(),
        }
    }
}

/// In-memory state of current goings-on exposed on /
#[derive(Clone, Serialize)]
pub struct State {
    #[serde(deserialize_with = "from_ts")]
    pub last_event: DateTime<Utc>,
}
impl State {
    fn new() -> Self {
        State {
            last_event: Utc::now()
        }
    }
}


/// User state for Actix
#[derive(Clone)]
pub struct Controller {
    /// An informer for Foo
    info: Informer<Foo>,
    /// In memory state
    state: Arc<RwLock<State>>,
    /// Various prometheus metrics
    metrics: Arc<RwLock<Metrics>>,
    /// A kube client for performing cluster actions based on Foo events
    client: APIClient,
}

/// Example Controller that watches Foos
///
/// This only deals with a single CRD, and it takes the NAMESPACE from an evar.
impl Controller {
    async fn new(client: APIClient) -> Result<Self> {
        let namespace = env::var("NAMESPACE").unwrap_or("default".into());
        let foos : Api<Foo> = Api::customResource(client.clone(), "foos")
            .version("v1")
            .group("clux.dev")
            .within(&namespace);
        let info = Informer::new(foos)
            .timeout(15)
            .init()
            .await?;
        let metrics = Arc::new(RwLock::new(Metrics::new()));
        let state = Arc::new(RwLock::new(State::new()));
        Ok(Controller { info, metrics, state, client })
    }
    /// Internal poll for internal thread
    async fn poll(&self) -> Result<()> {
        self.info.poll().await?;
        // in this example we always just handle all the events as they happen:
        while let Some(event) = self.info.pop() {
            self.handle_event(event)?;
        }
        Ok(())
    }

    fn handle_event(&self, ev: WatchEvent<Foo>) -> Result<()> {
        // This example only builds some debug data based on events
        // You can use self.client here to make the necessary kube api calls
        match ev {
            WatchEvent::Added(o) => {
                info!("Added Foo: {} ({})", o.metadata.name, o.spec.info);
            },
            WatchEvent::Modified(o) => {
                info!("Modified Foo: {} ({})", o.metadata.name, o.spec.info);
            },
            WatchEvent::Deleted(o) => {
                info!("Deleted Foo: {}", o.metadata.name);
            },
            WatchEvent::Error(e) => {
                warn!("Error event: {:?}", e); // we could refresh here
            }
        }
        self.metrics.write().unwrap().handled_events.inc();
        self.state.write().unwrap().last_event = Utc::now();

        Ok(())
    }
    /// Metrics getter
    pub fn metrics(&self) -> Vec<MetricFamily> {
        default_registry().gather()
    }
    /// State getter
    pub fn state(&self) -> Result<State> {
        // unwrap for users because Poison errors are not great to deal with atm
        // rather just have the handler 500 above in this case
        let res = self.state.read().unwrap().clone();
        Ok(res)
    }
}

/// Lifecycle initialization interface for app
///
/// This returns a `Controller` and calls `poll` on it continuously.
pub async fn init(cfg: Configuration) -> Result<Controller> {
    let c = Controller::new(APIClient::new(cfg)).await?; // for app to read
    let c2 = c.clone(); // for poll thread to write
    tokio::spawn(async move {
        loop {
            let _ = c2.poll().await.map_err(|e| {
                error!("Kube state failed to recover: {}", e);
                // rely on kube's crash loop backoff to retry sensibly:
                std::process::exit(1);
            });
        }
    });
    Ok(c)
}
