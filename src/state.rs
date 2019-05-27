use kube::{
    client::APIClient,
    config::Configuration,
    api::{Informer, WatchEvent, Api, Void},
};
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
pub struct FooResource {
  name: String,
  info: String,
}

/// Alias for inner state
pub type Cache = BTreeMap<String, FooResource>;

/// User state for Actix
#[derive(Clone)]
pub struct State {
    /// An informer for FooResource (with a blank Status struct)
    info: Informer<FooResource, Void>,
    /// Internal state built up by reconciliation loop
    cache: Arc<RwLock<Cache>>,
    /// A kube client for performing cluster actions based on Foo events
    client: APIClient,
}

/// Example State machine that watches
///
/// This only deals with a single CRD, and it takes the NAMESPACE from an evar.
impl State {
    fn new(client: APIClient) -> Result<Self> {
        let namespace = env::var("NAMESPACE").unwrap_or("kube-system".into());
        let fooresource = Api::customResource("foos")
            .version("v1")
            .group("clux.dev")
            .within(&namespace);
        let info = Informer::new(client.clone(), fooresource)
            .timeout(30)
            .init()?;
        let cache = Arc::new(RwLock::new(BTreeMap::new()));
        Ok(State { info, cache, client })
    }
    /// Internal poll for internal thread
    fn poll(&self) -> Result<()> {
        self.info.poll()?;
        // in this example we always just handle all the events as they happen:
        while let Some(event) = self.info.pop() {
            self.handle_event(event)?;
        }
        Ok(())
    }

    fn handle_event(&self, ev: WatchEvent<FooResource, Void>) -> Result<()> {
        // This example only builds up an internal map from the events
        // You can use self.client here to make the necessary kube api calls
        match ev {
            WatchEvent::Added(o) => {
                let name = o.metadata.name.clone();
                info!("Added Foo: {} ({})", name, o.spec.info);
                self.cache.write().unwrap()
                    .entry(name).or_insert_with(|| o.spec);
            },
            WatchEvent::Modified(o) => {
                let name = o.metadata.name.clone();
                info!("Modified Foo: {} ({})", name, o.spec.info);
                self.cache.write().unwrap()
                    .entry(name).and_modify(|e| *e = o.spec);
            },
            WatchEvent::Deleted(o) => {
                info!("Deleted Foo: {}", o.metadata.name);
                self.cache.write().unwrap()
                    .remove(&o.metadata.name);
            },
            WatchEvent::Error(e) => {
                warn!("Error event: {:?}", e); // we could refresh here
            }
        }
        Ok(())
    }
    /// Exposed getters for read access to state for app
    pub fn foos(&self) -> Result<Cache> {
        // unwrap for users because Poison errors are not great to deal with atm
        // rather just have the handler 500 above in this case
        let res = self.cache.read().unwrap().clone();
        Ok(res)
    }
}

/// Lifecycle initialization interface for app
///
/// This returns a `State` and calls `poll` on it continuously.
pub fn init(cfg: Configuration) -> Result<State> {
    let state = State::new(APIClient::new(cfg))?; // for app to read
    let state_clone = state.clone(); // clone for internal thread
    std::thread::spawn(move || {
        loop {
            state_clone.poll().map_err(|e| {
                error!("Kube state failed to recover: {}", e);
                // rely on kube's crash loop backoff to retry sensibly:
                std::process::exit(1);
            }).unwrap();
        }
    });
    Ok(state)
}
