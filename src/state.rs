use serde::de::DeserializeOwned;
use serde::Deserialize;
use failure::err_msg;
use log::{info, warn, error, debug, trace};

use kubernetes::{
    client::APIClient,
    config::Configuration,
    api::{Named, Cache, Reflector, ResourceMap, ApiResource},
};

use std::{
    collections::BTreeMap,
    env,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};

use crate::*;

/// Approximation of the CRD we want to work with
/// Replace with own struct.
/// Add serialize for returnability.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct FooResource {
  name: String,
  info: String,
}
use std::fmt::Debug;
impl Named for FooResource {
    // we want Foo identified by self.name in the cache
    fn name(&self) -> String {
        self.name.clone()
    }
}



// User state
#[derive(Clone)]
pub struct State {
    // Add resources you need in here
    foos: Reflector<FooResource>,
}

impl State {
    fn new(client: APIClient) -> Result<Self> {
        let namespace = env::var("NAMESPACE").expect("Need NAMESPACE evar");
        let fooresource = ApiResource {
            group: "clux.dev".into(),
            resource: "foos".into(),
            namespace: namespace,
        };
        let foos = Reflector::new(client, fooresource)?;
        Ok(State { foos })
    }

    // Internal poll for internal thread
    fn poll(&self) -> Result<()> {
        self.foos.poll()?;
        Ok(())
    }

    // Expose a full refresh button for the app
    pub fn refresh(&self) -> Result<()> {
        self.foos.refresh()?;
        Ok(())
    }

    // Expose a getter for the app
    pub fn foos(&self) -> Result<Cache<FooResource>> {
        Ok(self.foos.read()?)
    }
}

/// Initialise all data and start polling for changes
///
/// This returns a `State` and calls `poll` on it continuously.
/// As a result, this file encapsulates the only write access to a
pub fn init(cfg: Configuration) -> Result<State> {
    let client = APIClient::new(cfg);
    let state = State::new(client)?; // for webapp

    let state2 = state.clone(); // for internal thread
    // continuously poll for updates
    use std::thread;
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(10));
            // poll all reflectors here
            // (this can cause a few more waits in edge cases)
            match state2.poll() {
                Ok(_) => trace!("State refreshed"),
                Err(e) => {
                    // Bad fallback, but at least it leaves system working.
                    error!("Failed to refesh cache '{}' - rebooting", e);
                    std::process::exit(1);
                }
            }
        }
    });

    Ok(state)
}
