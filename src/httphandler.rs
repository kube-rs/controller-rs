use crate::{Error, Error::*, RhaiRes, get_client_name};
use actix_web::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::{Certificate, Client, Response, StatusCode};
use rhai::{Dynamic, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::runtime::Handle;
use tracing::*;

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
pub enum ReadMethod {
    #[default]
    Get,
}
#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
pub enum CreateMethod {
    #[default]
    Post,
    Put,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
pub enum UpdateMethod {
    #[default]
    Patch,
    Put,
    Post,
    None,
}

#[derive(Serialize, Deserialize, Eq, PartialEq, Clone, Debug, JsonSchema, Default)]
pub enum DeleteMethod {
    #[default]
    Delete,
}

#[derive(Clone, Debug)]
pub struct RestClient {
    baseurl: String,
    headers: Map,
    server_ca: Option<String>,
    client_key: Option<String>,
    client_cert: Option<String>,
}

impl RestClient {
    #[must_use]
    pub fn new(base: &str) -> Self {
        Self {
            baseurl: base.to_string(),
            headers: Map::new(),
            server_ca: None,
            client_cert: None,
            client_key: None,
        }
    }

    pub fn baseurl(&mut self, base: &str) -> &mut RestClient {
        self.baseurl = base.to_string();
        self
    }

    pub fn set_server_ca(&mut self, ca: &str) {
        self.server_ca = Some(ca.to_string());
    }

    pub fn set_mtls(&mut self, cert: &str, key: &str) {
        self.client_cert = Some(cert.to_string());
        self.client_key = Some(key.to_string());
    }

    pub fn baseurl_rhai(&mut self, base: String) {
        self.baseurl(base.as_str());
    }

    pub fn headers_reset(&mut self) -> &mut RestClient {
        self.headers = Map::new();
        self
    }

    pub fn headers_reset_rhai(&mut self) {
        self.headers_reset();
    }

    pub fn add_header(&mut self, key: &str, value: &str) -> &mut RestClient {
        self.headers
            .insert(key.to_string().into(), value.to_string().into());
        self
    }

    pub fn add_header_rhai(&mut self, key: String, value: String) {
        self.add_header(key.as_str(), value.as_str());
    }

    pub fn add_header_json_content(&mut self) -> &mut RestClient {
        if self
            .headers
            .clone()
            .into_iter()
            .any(|(c, _)| c == *"Content-Type")
        {
            self
        } else {
            self.add_header("Content-Type", "application/json; charset=utf-8")
        }
    }

    pub fn add_header_json_accept(&mut self) -> &mut RestClient {
        /*for (key, val) in self.headers.clone() {
            debug!("RestClient.header: {:} {:}", key, val);
        }*/
        if self.headers.clone().into_iter().any(|(c, _)| c == *"Accept") {
            self
        } else {
            self.add_header("Accept", "application/json")
        }
    }

    pub fn add_header_json(&mut self) {
        self.add_header_json_content().add_header_json_accept();
    }

    pub fn add_header_bearer(&mut self, token: &str) {
        self.add_header("Authorization", format!("Bearer {token}").as_str());
    }

    pub fn add_header_basic(&mut self, username: &str, password: &str) {
        let hash = STANDARD.encode(format!("{username}:{password}"));
        self.add_header("Authorization", format!("Basic {hash}").as_str());
    }

    fn get_client(&mut self) -> std::result::Result<Client, reqwest::Error> {
        let five_sec = std::time::Duration::from_secs(60 * 5);
        if self.server_ca.is_none() && (self.client_cert.is_none() || self.client_key.is_none()) {
            Client::builder()
                .user_agent(get_client_name())
                .timeout(five_sec)
                .build()
        } else if self.client_cert.is_none() || self.client_key.is_none() {
            match Certificate::from_pem(self.server_ca.clone().unwrap().as_bytes()) {
                Ok(c) => Client::builder()
                    .user_agent(get_client_name())
                    .timeout(five_sec)
                    .add_root_certificate(c)
                    .use_rustls_tls()
                    .build(),
                Err(e) => Err(e),
            }
        } else {
            let cli_cert = format!(
                "{}
{}",
                self.client_key.clone().unwrap(),
                self.client_cert.clone().unwrap()
            );
            match reqwest::Identity::from_pem(cli_cert.as_bytes()) {
                Ok(identity) => {
                    if self.server_ca.is_none() {
                        Client::builder()
                            .user_agent(get_client_name())
                            .timeout(five_sec)
                            .use_rustls_tls()
                            .identity(identity)
                            .build()
                    } else {
                        match Certificate::from_pem(self.server_ca.clone().unwrap().as_bytes()) {
                            Ok(c) => Client::builder()
                                .user_agent(get_client_name())
                                .timeout(five_sec)
                                .add_root_certificate(c)
                                .use_rustls_tls()
                                .identity(identity)
                                .build(),
                            Err(e) => Err(e),
                        }
                    }
                }
                Err(e) => Err(e),
            }
        }
    }

    pub fn http_get(&mut self, path: &str) -> std::result::Result<Response, reqwest::Error> {
        debug!("http_get '{}' ", format!("{}/{}", self.baseurl, path));
        match self.get_client() {
            Ok(client) => {
                let mut req = client.get(format!("{}/{}", self.baseurl, path));
                for (key, val) in self.headers.clone() {
                    req = req.header(key.to_string(), val.to_string());
                }
                tokio::task::block_in_place(|| Handle::current().block_on(async move { req.send().await }))
            }
            Err(e) => {
                if e.is_builder() {
                    warn!("CLIENT: {e:?}");
                }
                Err(e)
            }
        }
    }

    pub fn body_get(&mut self, path: &str) -> Result<String, Error> {
        let response = self.http_get(path).map_err(Error::ReqwestError)?;
        if !response.status().is_success() {
            let status = response.status();
            let text = tokio::task::block_in_place(|| {
                Handle::current().block_on(async move { response.text().await })
            })
            .map_err(Error::ReqwestError)?;
            return Err(Error::MethodFailed(
                "Get".to_string(),
                status.as_u16(),
                format!(
                    "The server returned the error: {} {} | {text}",
                    status.as_str(),
                    status.canonical_reason().unwrap_or("unknown")
                ),
            ));
        }
        let text =
            tokio::task::block_in_place(|| Handle::current().block_on(async move { response.text().await }))
                .map_err(Error::ReqwestError)?;
        Ok(text)
    }

    pub fn json_get(&mut self, path: &str) -> Result<Value, Error> {
        let text = self.body_get(path)?;
        let json = serde_json::from_str(&text).map_err(Error::JsonError)?;
        Ok(json)
    }

    pub fn rhai_get(&mut self, path: String) -> RhaiRes<Map> {
        let mut ret = Map::new();
        match self.http_get(path.as_str()) {
            Ok(result) => {
                ret.insert(
                    "code".to_string().into(),
                    Dynamic::from_int(result.status().as_u16().to_string().parse::<i64>().unwrap_or(0)),
                );
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let headers = result
                            .headers()
                            .into_iter()
                            .map(|(key, val)| {
                                (
                                    key.as_str().to_string(),
                                    val.to_str().unwrap_or_default().to_string(),
                                )
                            })
                            .collect::<Vec<(String, String)>>();
                        let text = match result.text().await {
                            Ok(t) => t,
                            Err(e) => {
                                ret.insert("body".to_string().into(), Dynamic::from(format!("Error reading response body: {e}")));
                                ret.insert("json".to_string().into(), Dynamic::from(json!({})));
                                ret.insert("headers".to_string().into(), Dynamic::from(headers));
                                return Err(format!("Error reading response body: {e}").into());
                            }
                        };
                        ret.insert(
                            "json".to_string().into(),
                            serde_json::from_str(&text).unwrap_or(Dynamic::from(json!({}))),
                        );
                        ret.insert("headers".to_string().into(), Dynamic::from(headers.clone()));
                        ret.insert("body".to_string().into(), Dynamic::from(text));
                        Ok(ret)
                    })
                })
            }
            Err(e) => Err(format!("{e}").into()),
        }
    }

    pub fn http_head(&mut self, path: &str) -> std::result::Result<Response, reqwest::Error> {
        debug!("http_get '{}' ", format!("{}/{}", self.baseurl, path));
        match self.get_client() {
            Ok(client) => {
                let mut req = client.head(format!("{}/{}", self.baseurl, path));
                for (key, val) in self.headers.clone() {
                    req = req.header(key.to_string(), val.to_string());
                }
                tokio::task::block_in_place(|| Handle::current().block_on(async move { req.send().await }))
            }
            Err(e) => {
                if e.is_builder() {
                    warn!("CLIENT: {e:?}");
                }
                Err(e)
            }
        }
    }

    pub fn rhai_head(&mut self, path: String) -> RhaiRes<Map> {
        let mut ret = Map::new();
        match self.http_head(path.as_str()) {
            Ok(result) => {
                ret.insert(
                    "code".to_string().into(),
                    Dynamic::from_int(result.status().as_u16().to_string().parse::<i64>().unwrap_or(0)),
                );
                let headers = result
                    .headers()
                    .into_iter()
                    .map(|(key, val)| {
                        (
                            key.as_str().to_string(),
                            val.to_str().unwrap_or_default().to_string(),
                        )
                    })
                    .collect::<Vec<(String, String)>>();
                ret.insert("headers".to_string().into(), Dynamic::from(headers.clone()));
                Ok(ret)
            }
            Err(e) => Err(format!("{e}").into()),
        }
    }

    pub fn http_patch(&mut self, path: &str, body: &str) -> Result<Response, reqwest::Error> {
        debug!("http_patch '{}' ", format!("{}/{}", self.baseurl, path));
        match self.get_client() {
            Ok(client) => {
                let mut req = client
                    .patch(format!("{}/{}", self.baseurl, path))
                    .body(body.to_string());
                for (key, val) in self.headers.clone() {
                    req = req.header(key.to_string(), val.to_string());
                }
                tokio::task::block_in_place(|| Handle::current().block_on(async move { req.send().await }))
            }
            Err(e) => Err(e),
        }
    }

    pub fn body_patch(&mut self, path: &str, body: &str) -> Result<String, Error> {
        let response = self.http_patch(path, body).map_err(Error::ReqwestError)?;
        if !response.status().is_success() {
            let status = response.status();
            let text = tokio::task::block_in_place(|| {
                Handle::current().block_on(async move { response.text().await })
            })
            .map_err(Error::ReqwestError)?;
            return Err(Error::MethodFailed(
                "Patch".to_string(),
                status.as_u16(),
                format!(
                    "The server returned the error: {} {} | {text}",
                    status.as_str(),
                    status.canonical_reason().unwrap_or("unknown")
                ),
            ));
        }
        let text =
            tokio::task::block_in_place(|| Handle::current().block_on(async move { response.text().await }))
                .map_err(Error::ReqwestError)?;
        Ok(text)
    }

    pub fn json_patch(&mut self, path: &str, input: &Value) -> Result<Value, Error> {
        let body = serde_json::to_string(input).map_err(Error::JsonError)?;
        let text = self.body_patch(path, body.as_str())?;
        let json = serde_json::from_str(&text).map_err(Error::JsonError)?;
        Ok(json)
    }

    pub fn rhai_patch(&mut self, path: String, val: Dynamic) -> RhaiRes<Map> {
        let body = if val.is_string() {
            val.to_string()
        } else {
            match serde_json::to_string(&val) {
                Ok(s) => s,
                Err(e) => return Err(format!("Failed to serialize body: {e}").into()),
            }
        };
        let mut ret = Map::new();
        match self.http_patch(path.as_str(), &body) {
            Ok(result) => {
                ret.insert(
                    "code".to_string().into(),
                    Dynamic::from_int(result.status().as_u16().to_string().parse::<i64>().unwrap_or(0)),
                );
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let headers = result
                            .headers()
                            .into_iter()
                            .map(|(key, val)| {
                                (
                                    key.as_str().to_string(),
                                    val.to_str().unwrap_or_default().to_string(),
                                )
                            })
                            .collect::<Vec<(String, String)>>();
                        let text = match result.text().await {
                            Ok(t) => t,
                            Err(e) => {
                                ret.insert("body".to_string().into(), Dynamic::from(format!("Error reading response body: {e}")));
                                ret.insert("json".to_string().into(), Dynamic::from(json!({})));
                                ret.insert("headers".to_string().into(), Dynamic::from(headers));
                                return Err(format!("Error reading response body: {e}").into());
                            }
                        };
                        ret.insert(
                            "json".to_string().into(),
                            serde_json::from_str(&text).unwrap_or(Dynamic::from(json!({}))),
                        );
                        ret.insert("headers".to_string().into(), Dynamic::from(headers.clone()));
                        ret.insert("body".to_string().into(), Dynamic::from(text));
                        Ok(ret)
                    })
                })
            }
            Err(e) => Err(format!("{e}").into()),
        }
    }

    pub fn http_put(&mut self, path: &str, body: &str) -> Result<Response, reqwest::Error> {
        debug!("http_put '{}' ", format!("{}/{}", self.baseurl, path));
        match self.get_client() {
            Ok(client) => {
                let mut req = client
                    .put(format!("{}/{}", self.baseurl, path))
                    .body(body.to_string());
                for (key, val) in self.headers.clone() {
                    req = req.header(key.to_string(), val.to_string());
                }
                tokio::task::block_in_place(|| Handle::current().block_on(async move { req.send().await }))
            }
            Err(e) => Err(e),
        }
    }

    pub fn body_put(&mut self, path: &str, body: &str) -> Result<String, Error> {
        let response = self.http_put(path, body).map_err(Error::ReqwestError)?;
        if !response.status().is_success() {
            let status = response.status();
            let text = tokio::task::block_in_place(|| {
                Handle::current().block_on(async move { response.text().await })
            })
            .map_err(Error::ReqwestError)?;
            return Err(Error::MethodFailed(
                "Put".to_string(),
                status.as_u16(),
                format!(
                    "The server returned the error: {} {} | {text}",
                    status.as_str(),
                    status.canonical_reason().unwrap_or("unknown")
                ),
            ));
        }
        let text =
            tokio::task::block_in_place(|| Handle::current().block_on(async move { response.text().await }))
                .map_err(Error::ReqwestError)?;
        Ok(text)
    }

    pub fn json_put(&mut self, path: &str, input: &Value) -> Result<Value, Error> {
        let body = serde_json::to_string(input).map_err(Error::JsonError)?;
        let text = self.body_put(path, body.as_str())?;
        let json = serde_json::from_str(&text).map_err(Error::JsonError)?;
        Ok(json)
    }

    pub fn rhai_put(&mut self, path: String, val: Dynamic) -> RhaiRes<Map> {
        let body = if val.is_string() {
            val.to_string()
        } else {
            match serde_json::to_string(&val) {
                Ok(s) => s,
                Err(e) => return Err(format!("Failed to serialize body: {e}").into()),
            }
        };
        let mut ret = Map::new();
        match self.http_put(path.as_str(), &body) {
            Ok(result) => {
                ret.insert(
                    "code".to_string().into(),
                    Dynamic::from_int(result.status().as_u16().to_string().parse::<i64>().unwrap_or(0)),
                );
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let headers = result
                            .headers()
                            .into_iter()
                            .map(|(key, val)| {
                                (
                                    key.as_str().to_string(),
                                    val.to_str().unwrap_or_default().to_string(),
                                )
                            })
                            .collect::<Vec<(String, String)>>();
                        let text = match result.text().await {
                            Ok(t) => t,
                            Err(e) => {
                                ret.insert("body".to_string().into(), Dynamic::from(format!("Error reading response body: {e}")));
                                ret.insert("json".to_string().into(), Dynamic::from(json!({})));
                                ret.insert("headers".to_string().into(), Dynamic::from(headers));
                                return Err(format!("Error reading response body: {e}").into());
                            }
                        };
                        ret.insert(
                            "json".to_string().into(),
                            serde_json::from_str(&text).unwrap_or(Dynamic::from(json!({}))),
                        );
                        ret.insert("headers".to_string().into(), Dynamic::from(headers.clone()));
                        ret.insert("body".to_string().into(), Dynamic::from(text));
                        Ok(ret)
                    })
                })
            }
            Err(e) => Err(format!("{e}").into()),
        }
    }

    pub fn http_post(&mut self, path: &str, body: &str) -> Result<Response, reqwest::Error> {
        debug!("http_post '{}' ", format!("{}/{}", self.baseurl, path));
        match self.get_client() {
            Ok(client) => {
                let mut req = client
                    .post(format!("{}/{}", self.baseurl, path))
                    .body(body.to_string());
                for (key, val) in self.headers.clone() {
                    req = req.header(key.to_string(), val.to_string());
                }
                tokio::task::block_in_place(|| Handle::current().block_on(async move { req.send().await }))
            }
            Err(e) => Err(e),
        }
    }

    pub fn body_post(&mut self, path: &str, body: &str) -> Result<String, Error> {
        let response = self.http_post(path, body).map_err(Error::ReqwestError)?;
        if !response.status().is_success() {
            let status = response.status();
            let text = tokio::task::block_in_place(|| {
                Handle::current().block_on(async move { response.text().await })
            })
            .map_err(Error::ReqwestError)?;
            return Err(Error::MethodFailed(
                "Post".to_string(),
                status.as_u16(),
                format!(
                    "The server returned the error: {} {} | {text}",
                    status.as_str(),
                    status.canonical_reason().unwrap_or("unknown")
                ),
            ));
        }
        let text =
            tokio::task::block_in_place(|| Handle::current().block_on(async move { response.text().await }))
                .map_err(Error::ReqwestError)?;
        Ok(text)
    }

    pub fn json_post(&mut self, path: &str, input: &Value) -> Result<Value, Error> {
        let body = serde_json::to_string(input).map_err(Error::JsonError)?;
        let text = self.body_post(path, body.as_str())?;
        let json = serde_json::from_str(&text).map_err(Error::JsonError)?;
        Ok(json)
    }

    pub fn rhai_post(&mut self, path: String, val: Dynamic) -> RhaiRes<Map> {
        let body = if val.is_string() {
            val.to_string()
        } else {
            match serde_json::to_string(&val) {
                Ok(s) => s,
                Err(e) => return Err(format!("Failed to serialize body: {e}").into()),
            }
        };
        let mut ret = Map::new();
        match self.http_post(path.as_str(), &body) {
            Ok(result) => {
                ret.insert(
                    "code".to_string().into(),
                    Dynamic::from_int(result.status().as_u16().to_string().parse::<i64>().unwrap_or(0)),
                );
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let headers = result
                            .headers()
                            .into_iter()
                            .map(|(key, val)| {
                                (
                                    key.as_str().to_string(),
                                    val.to_str().unwrap_or_default().to_string(),
                                )
                            })
                            .collect::<Vec<(String, String)>>();
                        let text = match result.text().await {
                            Ok(t) => t,
                            Err(e) => {
                                ret.insert("body".to_string().into(), Dynamic::from(format!("Error reading response body: {e}")));
                                ret.insert("json".to_string().into(), Dynamic::from(json!({})));
                                ret.insert("headers".to_string().into(), Dynamic::from(headers));
                                return Err(format!("Error reading response body: {e}").into());
                            }
                        };
                        ret.insert(
                            "json".to_string().into(),
                            serde_json::from_str(&text).unwrap_or(Dynamic::from(json!({}))),
                        );
                        ret.insert("headers".to_string().into(), Dynamic::from(headers.clone()));
                        ret.insert("body".to_string().into(), Dynamic::from(text));
                        Ok(ret)
                    })
                })
            }
            Err(e) => Err(format!("{e}").into()),
        }
    }

    pub fn http_post_form(
        &mut self,
        path: &str,
        params: &[(String, String)],
    ) -> Result<Response, reqwest::Error> {
        debug!("http_post_form '{}' ", format!("{}/{}", self.baseurl, path));
        match self.get_client() {
            Ok(client) => {
                let mut req = client.post(format!("{}/{}", self.baseurl, path)).form(params);
                for (key, val) in self.headers.clone() {
                    if key.as_str() != "Content-Type" {
                        req = req.header(key.to_string(), val.to_string());
                    }
                }
                tokio::task::block_in_place(|| Handle::current().block_on(async move { req.send().await }))
            }
            Err(e) => Err(e),
        }
    }

    pub fn rhai_post_form(&mut self, path: String, val: Map) -> RhaiRes<Map> {
        let params: Vec<(String, String)> = val
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let mut ret = Map::new();
        match self.http_post_form(path.as_str(), &params) {
            Ok(result) => {
                ret.insert(
                    "code".to_string().into(),
                    Dynamic::from_int(result.status().as_u16().to_string().parse::<i64>().unwrap_or(0)),
                );
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let headers = result
                            .headers()
                            .into_iter()
                            .map(|(key, val)| {
                                (
                                    key.as_str().to_string(),
                                    val.to_str().unwrap_or_default().to_string(),
                                )
                            })
                            .collect::<Vec<(String, String)>>();
                        let text = match result.text().await {
                            Ok(t) => t,
                            Err(e) => {
                                ret.insert("body".to_string().into(), Dynamic::from(format!("Error reading response body: {e}")));
                                ret.insert("json".to_string().into(), Dynamic::from(json!({})));
                                ret.insert("headers".to_string().into(), Dynamic::from(headers));
                                return Err(format!("Error reading response body: {e}").into());
                            }
                        };
                        ret.insert(
                            "json".to_string().into(),
                            serde_json::from_str(&text).unwrap_or(Dynamic::from(json!({}))),
                        );
                        ret.insert("headers".to_string().into(), Dynamic::from(headers.clone()));
                        ret.insert("body".to_string().into(), Dynamic::from(text));
                        Ok(ret)
                    })
                })
            }
            Err(e) => Err(format!("{e}").into()),
        }
    }

    pub fn http_delete(&mut self, path: &str) -> Result<Response, reqwest::Error> {
        debug!("http_delete '{}' ", format!("{}/{}", self.baseurl, path));
        match self.get_client() {
            Ok(client) => {
                let mut req = client.delete(format!("{}/{}", self.baseurl, path));
                for (key, val) in self.headers.clone() {
                    req = req.header(key.to_string(), val.to_string());
                }
                tokio::task::block_in_place(|| Handle::current().block_on(async move { req.send().await }))
            }
            Err(e) => Err(e),
        }
    }

    pub fn body_delete(&mut self, path: &str) -> Result<String, Error> {
        let response = self.http_delete(path).map_err(Error::ReqwestError)?;
        if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
            let status = response.status();
            let text = tokio::task::block_in_place(|| {
                Handle::current().block_on(async move { response.text().await })
            })
            .map_err(Error::ReqwestError)?;
            return Err(Error::MethodFailed(
                "Delete".to_string(),
                status.as_u16(),
                format!(
                    "The server returned the error: {} {} | {text}",
                    status.as_str(),
                    status.canonical_reason().unwrap_or("unknown")
                ),
            ));
        }
        let text =
            tokio::task::block_in_place(|| Handle::current().block_on(async move { response.text().await }))
                .map_err(Error::ReqwestError)?;
        Ok(text)
    }

    pub fn json_delete(&mut self, path: &str) -> Result<Value, Error> {
        let text = self.body_delete(path)?;
        let json =
            serde_json::from_str(&text).or_else(|_| Ok::<serde_json::Value, Error>(json!({"body": text})))?;
        Ok(json)
    }

    pub fn http_delete_with_body(&mut self, path: &str, body: &str) -> Result<Response, reqwest::Error> {
        debug!(
            "http_delete_with_body '{}' ",
            format!("{}/{}", self.baseurl, path)
        );
        match self.get_client() {
            Ok(client) => {
                let mut req = client
                    .delete(format!("{}/{}", self.baseurl, path))
                    .body(body.to_string());
                for (key, val) in self.headers.clone() {
                    req = req.header(key.to_string(), val.to_string());
                }
                tokio::task::block_in_place(|| Handle::current().block_on(async move { req.send().await }))
            }
            Err(e) => Err(e),
        }
    }

    pub fn body_delete_with_body(&mut self, path: &str, body: &str) -> Result<String, Error> {
        let response = self
            .http_delete_with_body(path, body)
            .map_err(Error::ReqwestError)?;
        if !response.status().is_success() && response.status() != StatusCode::NOT_FOUND {
            let status = response.status();
            let text = tokio::task::block_in_place(|| {
                Handle::current().block_on(async move { response.text().await })
            })
            .map_err(Error::ReqwestError)?;
            return Err(Error::MethodFailed(
                "Delete".to_string(),
                status.as_u16(),
                format!(
                    "The server returned the error: {} {} | {text}",
                    status.as_str(),
                    status.canonical_reason().unwrap_or("unknown")
                ),
            ));
        }
        let text =
            tokio::task::block_in_place(|| Handle::current().block_on(async move { response.text().await }))
                .map_err(Error::ReqwestError)?;
        Ok(text)
    }

    pub fn json_delete_with_body(&mut self, path: &str, input: &Value) -> Result<Value, Error> {
        let body = serde_json::to_string(input).map_err(Error::JsonError)?;
        let text = self.body_delete_with_body(path, body.as_str())?;
        let json =
            serde_json::from_str(&text).or_else(|_| Ok::<serde_json::Value, Error>(json!({"body": text})))?;
        Ok(json)
    }

    pub fn rhai_delete(&mut self, path: String) -> RhaiRes<Map> {
        let mut ret = Map::new();
        match self.http_delete(path.as_str()) {
            Ok(result) => {
                ret.insert(
                    "code".to_string().into(),
                    Dynamic::from_int(result.status().as_u16().to_string().parse::<i64>().unwrap_or(0)),
                );
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let headers = result
                            .headers()
                            .into_iter()
                            .map(|(key, val)| {
                                (
                                    key.as_str().to_string(),
                                    val.to_str().unwrap_or_default().to_string(),
                                )
                            })
                            .collect::<Vec<(String, String)>>();
                        let text = match result.text().await {
                            Ok(t) => t,
                            Err(e) => {
                                ret.insert("body".to_string().into(), Dynamic::from(format!("Error reading response body: {e}")));
                                ret.insert("json".to_string().into(), Dynamic::from(json!({})));
                                ret.insert("headers".to_string().into(), Dynamic::from(headers));
                                return Err(format!("Error reading response body: {e}").into());
                            }
                        };
                        ret.insert(
                            "json".to_string().into(),
                            serde_json::from_str(&text).unwrap_or(Dynamic::from(json!({}))),
                        );
                        ret.insert("headers".to_string().into(), Dynamic::from(headers.clone()));
                        ret.insert("body".to_string().into(), Dynamic::from(text));
                        Ok(ret)
                    })
                })
            }
            Err(e) => Err(format!("{e}").into()),
        }
    }

    pub fn rhai_delete_with_body(&mut self, path: String, val: Dynamic) -> RhaiRes<Map> {
        let body = if val.is_string() {
            val.to_string()
        } else {
            match serde_json::to_string(&val) {
                Ok(s) => s,
                Err(e) => return Err(format!("Failed to serialize body: {e}").into()),
            }
        };
        let mut ret = Map::new();
        match self.http_delete_with_body(path.as_str(), &body) {
            Ok(result) => {
                ret.insert(
                    "code".to_string().into(),
                    Dynamic::from_int(result.status().as_u16().to_string().parse::<i64>().unwrap_or(0)),
                );
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let headers = result
                            .headers()
                            .into_iter()
                            .map(|(key, val)| {
                                (
                                    key.as_str().to_string(),
                                    val.to_str().unwrap_or_default().to_string(),
                                )
                            })
                            .collect::<Vec<(String, String)>>();
                        let text = match result.text().await {
                            Ok(t) => t,
                            Err(e) => {
                                ret.insert("body".to_string().into(), Dynamic::from(format!("Error reading response body: {e}")));
                                ret.insert("json".to_string().into(), Dynamic::from(json!({})));
                                ret.insert("headers".to_string().into(), Dynamic::from(headers));
                                return Err(format!("Error reading response body: {e}").into());
                            }
                        };
                        ret.insert(
                            "json".to_string().into(),
                            serde_json::from_str(&text).unwrap_or(Dynamic::from(json!({}))),
                        );
                        ret.insert("headers".to_string().into(), Dynamic::from(headers.clone()));
                        ret.insert("body".to_string().into(), Dynamic::from(text));
                        Ok(ret)
                    })
                })
            }
            Err(e) => Err(format!("{e}").into()),
        }
    }

    pub fn obj_read(&mut self, method: ReadMethod, path: &str, key: &str) -> Result<Value, Error> {
        let full_path = if key.is_empty() {
            path.to_string()
        } else {
            format!("{path}/{key}")
        };
        if method == ReadMethod::Get {
            self.json_get(&full_path)
        } else {
            Err(UnsupportedMethod)
        }
    }

    pub fn obj_create(&mut self, method: CreateMethod, path: &str, input: &Value) -> Result<Value, Error> {
        if method == CreateMethod::Post {
            self.json_post(path, input)
        } else if method == CreateMethod::Put {
            self.json_put(path, input)
        } else {
            Err(UnsupportedMethod)
        }
    }

    pub fn obj_update(
        &mut self,
        method: UpdateMethod,
        path: &str,
        key: &str,
        input: &Value,
    ) -> Result<Value, Error> {
        let full_path = if key.is_empty() {
            path.to_string()
        } else {
            format!("{path}/{key}")
        };
        if method == UpdateMethod::Patch {
            self.json_patch(&full_path, input)
        } else if method == UpdateMethod::Put {
            self.json_put(&full_path, input)
        } else if method == UpdateMethod::Post {
            self.json_post(&full_path, input)
        } else if method == UpdateMethod::None {
            Ok(input.clone())
        } else {
            Err(UnsupportedMethod)
        }
    }

    pub fn obj_delete(&mut self, method: DeleteMethod, path: &str, key: &str) -> Result<Value, Error> {
        let full_path = if key.is_empty() {
            path.to_string()
        } else {
            format!("{path}/{key}")
        };
        if method == DeleteMethod::Delete {
            self.json_delete(&full_path)
        } else {
            Err(UnsupportedMethod)
        }
    }

    pub fn obj_delete_with_body(
        &mut self,
        method: DeleteMethod,
        path: &str,
        input: &Value,
    ) -> Result<Value, Error> {
        if method == DeleteMethod::Delete {
            self.json_delete_with_body(path, input)
        } else {
            Err(UnsupportedMethod)
        }
    }
}
