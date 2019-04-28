#![allow(non_snake_case)]

use super::{Result, Error};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Resource {
    /// API Resource name
    pub resource: String,
    /// API Group
    pub group: String,
    /// Namespace the resources reside
    pub namespace: String,
}

/// Create a list request for a Resource
///
/// Useful to fully re-fetch the state.
fn list_all_crd_entries(r: &Resource) -> Result<http::Request<Vec<u8>>> {
    let urlstr = format!("/apis/{group}/v1/namespaces/{ns}/{resource}?",
        group = r.group, resource = r.resource, ns = r.namespace);
    let urlstr = url::form_urlencoded::Serializer::new(urlstr).finish();
    let mut req = http::Request::get(urlstr);
    req.body(vec![]).map_err(Error::from)
}


/// Create watch request for a Resource at a given resourceVer
///
/// Should be used continuously
fn watch_crd_entries_after(r: &Resource, ver: &str) -> Result<http::Request<Vec<u8>>> {
    let urlstr = format!("/apis/{group}/v1/namespaces/{ns}/{resource}?",
        group = r.group, resource = r.resource, ns = r.namespace);
    let mut qp = url::form_urlencoded::Serializer::new(urlstr);

    qp.append_pair("timeoutSeconds", "10");
    qp.append_pair("watch", "true");
    qp.append_pair("resourceVersion", ver);

    let urlstr = qp.finish();
    let mut req = http::Request::get(urlstr);
    req.body(vec![]).map_err(Error::from)
}

// below actually uses request objects


// cruft
use std::fmt::Debug;
use std::fmt::Display;

//use serde::Deserialize;
use serde::{de, Deserialize, Deserializer};
use std::str::FromStr;
use serde::de::DeserializeOwned;
use serde::Serialize;
pub trait Named {
    fn name(&self) -> String;
}

/// ApiError for when things fail
///
/// This can be parsed into as a fallback in various places
/// `WatchEvents` has a particularly egregious use of it.
#[derive(Deserialize, Debug)]
pub struct ApiError {
    status: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    reason: Option<String>,
    code: u16,
}

/// Events from a watch query
///
/// Should expect a one of these per line from `watch_crd_entries_after`
/// TODO: think this is always adjacently tagged, at least it is for Error...
/// TODO: reuse between other watch api by not hardcoding Crd<T>
#[derive(Deserialize, Debug)]
#[serde(tag = "type", content = "object", rename_all = "UPPERCASE")]
pub enum WatchEvent<T> where
  T: Debug + Clone
{
    Added(T),
    Modified(T),
    Deleted(T),
    Error(ApiError),
}



/// Basic CRD wrapper struct
///
/// Expected to be used by `CrdList` and `WatchEvent`
#[derive(Deserialize, Debug, Clone)]
pub struct Crd<T> where
  T: Debug + Clone + Named
{
    pub apiVersion: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: T,
}


/// Basic Metadata struct
///
/// Only parses a few fields relevant to a reflector.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Metadata {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
    // TODO: generation?
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resourceVersion: String,
}

/// Basic CRD List
///
/// Expected to be returned by a query from `list_all_crd_entries`
#[derive(Deserialize)]
pub struct CrdList<T> where
  T: Debug + Clone
{
    pub apiVersion: String,
    pub kind: String,
    pub metadata: Metadata,
    #[serde(bound(deserialize = "Vec<T>: Deserialize<'de>"))]
    pub items: Vec<T>,
}

use kubernetes::client::APIClient;

pub type ResourceMap<T> = BTreeMap<String, T>;

#[derive(Default, Clone)]
pub struct Cache<T> {
    pub data: ResourceMap<T>,
    /// Current resourceVersion used for bookkeeping
    version: String,
}


pub fn get_cr_entries<T>(client: &APIClient, rg: &Resource) -> Result<Cache<T>> where
  T: Debug + Clone + Named + DeserializeOwned
{
    let req = list_all_crd_entries(&rg)?;
    let res = client.request::<CrdList<Crd<T>>>(req)?;
    let mut data = BTreeMap::new();
    let version = res.metadata.resourceVersion;
    info!("Got {} with {} elements at resourceVersion={}", res.kind, res.items.len(), version);

    for i in res.items {
        data.insert(i.spec.name(), i.spec);
    }
    let keys = data.keys().cloned().collect::<Vec<_>>().join(", ");
    debug!("Initialized with: {}", keys);
    Ok(Cache { data, version })
}

pub fn watch_for_cr_updates<T>(client: &APIClient, rg: &Resource, mut c: Cache<T>) -> Result<Cache<T>>
  where
  T: Debug + Clone + Named + DeserializeOwned
{
    let req = watch_crd_entries_after(&rg, &c.version)?;
    let res = client.request_events::<WatchEvent<Crd<T>>>(req)?;


    // TODO: let c.version == max of individual versions?
    // probably better, but api docs says not to rely on format of it...
    for ev in res {
        debug!("Got {:?}", ev);
        match ev {
            WatchEvent::Added(o) => {
                info!("Adding service {}", o.spec.name());
                c.data.entry(o.spec.name().clone())
                    .or_insert_with(|| o.spec.clone());
                if o.metadata.resourceVersion != "" {
                  c.version = o.metadata.resourceVersion.clone();
                }
            },
            WatchEvent::Modified(o) => {
                info!("Modifying service {}", o.spec.name());
                c.data.entry(o.spec.name().clone())
                    .and_modify(|e| *e = o.spec.clone());
                if o.metadata.resourceVersion != "" {
                  c.version = o.metadata.resourceVersion.clone();
                }
            },
            WatchEvent::Deleted(o) => {
                info!("Removing service {}", o.spec.name());
                c.data.remove(&o.spec.name());
                if o.metadata.resourceVersion != "" {
                  c.version = o.metadata.resourceVersion.clone();
                }
            }
            WatchEvent::Error(e) => {
                warn!("Failed to watch resource: {:?}", e)
            }
        }
    }
    //debug!("Updated: {}", found.join(", "));
    Ok(c) // updated in place (taken ownership)
}

