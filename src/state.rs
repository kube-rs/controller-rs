use log::{info, warn, error, debug, trace};
use kube::{
    client::APIClient,
    config::Configuration,
    api::{ReflectorSpec, Reflector, ResourceMap, ResourceSpecMap, ApiResource},
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
pub struct DeploymentSpec {
    replicas: i32
}
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct DeploymentStatus {
    availableReplicas: i32
}

// You can also use the full structs in a reflector
use k8s_openapi::api::apps::v1::DeploymentStatus as FullStatus;
use k8s_openapi::api::apps::v1::Deployment as FullDeploy;

/// User state for Actix
#[derive(Clone)]
pub struct State {
    // Add resources you need in here, expose it as you see fit
    // this example encapsulates it behind a getter and internal poll thread below.
    foos: ReflectorSpec<FooResource>,
    /// You can also have reflectors for normal resources
    deploys: Reflector<DeploymentSpec, DeploymentStatus>,
    /// Full deploy data using openapi spec
    deploysfull: Reflector<FullDeploy, FullStatus>,
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
            version: "v1".into(),
            namespace: Some(namespace.clone()),
        };
        let foos = Reflector::new(client.clone(), fooresource)?;
        let deployresource = ApiResource {
            group: "apps".into(),
            resource: "deployments".into(),
            version: "v1".into(),
            namespace: Some(namespace.clone()),
        };
        let deploys = Reflector::new(client.clone(), deployresource)?;
        let deployresourcefull = ApiResource {
            group: "apps".into(),
            resource: "deployments".into(),
            version: "v1".into(),
            namespace: Some(namespace.clone()),
        };
        let deploysfull = Reflector::new(client, deployresourcefull)?;
        Ok(State { foos, deploys, deploysfull })
    }
    /// Internal poll for internal thread
    fn poll(&self) -> Result<()> {
        self.foos.poll()?;
        self.deploys.poll()?;
        self.deploysfull.poll()?;
        Ok(())
    }
    /// Exposed refresh button for use by app
    pub fn refresh(&self) -> Result<()> {
        self.foos.refresh()?;
        self.deploys.refresh()?;
        self.deploysfull.refresh()?;
        Ok(())
    }
    /// Exposed getters for read access to state for app
    pub fn foos(&self) -> Result<ResourceSpecMap<FooResource>> {
        self.foos.read()
    }
    pub fn deploys(&self) -> Result<ResourceMap<DeploymentSpec, DeploymentStatus>> {
        self.deploys.read()
    }
    pub fn deploysfull(&self) -> Result<ResourceMap<FullDeploy, FullStatus>> {
        self.deploysfull.read()
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
