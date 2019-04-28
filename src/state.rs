use serde::de::DeserializeOwned;
use serde::Deserialize;
use failure::err_msg;
use log::{info, warn, error, debug, trace};

use kubernetes::{
    client::APIClient,
    config::Configuration,
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
#[derive(Debug, Deserialize, Clone)]
pub struct FooResource {
  name: String,
}
use kube::{Named, Cache};
use std::fmt::Debug;
impl Named for FooResource {
    // we want Foo identified by self.name in the cache
    fn name(&self) -> String {
        self.name.clone()
    }
}



// generic stuff
#[derive(Clone)]
pub struct Reflector<T> where
  T: Debug + Clone + Named
{
    /// Application state can be read continuously with read
    ///
    /// Write access to this data is entirely encapsulated within poll + refresh
    /// Users are meant to start a thread to poll, and maybe ask for a refresh.
    /// Beyond that, use the read call as a local cache.
    data: Arc<RwLock<Cache<T>>>,

    /// Kubernetes API Client
    client: APIClient,

    /// Resource this Reflector is responsible for
    resource: kube::Resource,
}

impl<T> Reflector<T> where
    T: Debug + Clone + Named + DeserializeOwned
{
    /// Create a reflector with a kube client on a kube resource
    ///
    /// Initializes with a full list of data from a large initial LIST call
    pub fn new(client: APIClient, r: kube::Resource) -> Result<Self> {
        info!("Creating Reflector for {:?}", r);
        let current : Cache<T> = kube::get_cr_entries(&client, &r)?;
        Ok(Reflector {
            client,
            resource: r,
            data: Arc::new(RwLock::new(current)),
        })
    }

    /// Run a single watch poll
    ///
    /// If this returns an error, it tries a full refresh.
    /// This is meant to be run continually in a thread. Spawn one.
    pub fn poll(&self) -> Result<()> {
        use std::thread;
        trace!("Watching {:?}", self.resource);
        let old = self.data.read().unwrap().clone();
        match kube::watch_for_cr_updates(&self.client, &self.resource, old) {
            Ok(res) => {
                *self.data.write().unwrap() = res;
            },
            Err(e) => {
                // If desynched due to mismatching resourceVersion, retry in a bit
                thread::sleep(Duration::from_secs(10));
                self.refresh()?; // propagate error if this failed..
            }
        }

        Ok(())
    }

    /// Read data for users of the reflector
    pub fn read(&self) -> Result<Cache<T>> {
        // unwrap for users because Poison errors are not great to deal with atm.
        // If a read fails, you've probably failed to parse the Resource into a T
        // this likely implies versioning issues between:
        // - your definition of T (in code used to instantiate Reflector)
        // - current applied kube state (used to parse into T)
        //
        // Very little that can be done in this case. Upgrade your app / resource.
        let data = self.data.read().unwrap().clone();
        Ok(data)
    }

    /// Refresh the full resource state with a LIST call
    ///
    /// Same as what is done in `State::new`.
    pub fn refresh(&self) -> Result<()> {
        debug!("Refreshing {:?}", self.resource);
        let current : Cache<T> = kube::get_cr_entries(&self.client, &self.resource)?;
        *self.data.write().unwrap() = current;
        Ok(())
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
        let fooresource = kube::Resource {
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
