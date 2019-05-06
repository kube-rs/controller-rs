use log::{info, warn, error, debug, trace};
use kube::{
    client::APIClient,
    config::Configuration,
    api::{Reflector, ResourceMap, ApiResource},
};
use std::{
    env,
    time::Duration,
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

/// Kubernetes Deployment simplified
/// Just the parts we care about
/// Use k8s-openapi for full structs
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Deployment {
    replicas: i32
}

/// User state for Actix
#[derive(Clone)]
pub struct State {
    // Add resources you need in here, expose it as you see fit
    // this example encapsulates it behind a getter and internal poll thread below.
    foos: Reflector<FooResource>,
    /// You can also have reflectors for normal resources
    deploys: Reflector<Deployment>,
}

/// Example state machine that exposes the state of one `Reflector<FooResource>`
///
/// This only deals with a single CRD, and it takes the NAMESPACE from an evar.
impl State {
    fn new(client: APIClient) -> Result<Self> {
        let namespace = env::var("NAMESPACE").unwrap_or("kube-system".into());
        let fooresource = ApiResource {
            group: "clux.dev".into(),
            resource: "foos".into(),
            namespace: Some(namespace.clone()),
        };
        let foos = Reflector::new(client.clone(), fooresource)?;
        let deployresource = ApiResource {
            group: "apps".into(),
            resource: "deployments".into(),
            namespace: Some(namespace.clone()),
        };
        let deploys = Reflector::new(client, deployresource)?;
        Ok(State { foos, deploys })
    }
    /// Internal poll for internal thread
    fn poll(&self) -> Result<()> {
        self.foos.poll()?;
        self.deploys.poll()?;
        Ok(())
    }
    /// Exposed refresh button for use by app
    pub fn refresh(&self) -> Result<()> {
        self.foos.refresh()?;
        self.deploys.refresh()?;
        Ok(())
    }
    /// Exposed getters for read access to state for app
    pub fn foos(&self) -> Result<ResourceMap<FooResource>> {
        self.foos.read()
    }
    pub fn deploys(&self) -> Result<ResourceMap<Deployment>> {
        self.deploys.read()
    }
}

/// Lifecycle initialization interface for app
///
/// This returns a `State` and calls `poll` on it continuously.
/// As a result, this file encapsulates the only write access to a
pub fn init(cfg: Configuration) -> Result<State> {
    let state = State::new(APIClient::new(cfg))?; // for app to read
    let state_clone = state.clone(); // clone for internal thread
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(10));
            // update state here - can cause a few more waits in edge cases
            match state_clone.poll() {
                Ok(_) => trace!("State refreshed"), // normal case
                Err(e) => {
                    // Can't recover: boot as much as kubernetes' backoff allows
                    error!("Failed to refesh cache '{}' - rebooting", e);
                    std::process::exit(1); // boot might fix it if network is failing
                }
            }
        }
    });
    Ok(state)
}
