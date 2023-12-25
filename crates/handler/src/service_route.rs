use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use graphgate_planner::{Request, Response};
use http::HeaderMap;
use once_cell::sync::Lazy;
use tracing::instrument;

static HTTP_CLIENT: Lazy<reqwest::Client> = Lazy::new(Default::default);

/// Service routing information.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct ServiceRoute {
    /// Service address
    ///
    /// For example: 1.2.3.4:8000, example.com:8080
    pub addr: String,

    /// Use TLS
    pub tls: bool,

    /// GraphQL HTTP path, default is `/`.
    pub query_path: Option<String>,

    /// GraphQL WebSocket path, default is `/`.
    pub subscribe_path: Option<String>,

    pub introspection_path: Option<String>,

    pub websocket_path: Option<String>,
}

/// Service routing table
///
/// The key is the service name.
#[derive(Default, Debug, Clone, Eq, PartialEq)]
pub struct ServiceRouteTable(HashMap<String, ServiceRoute>);

impl Deref for ServiceRouteTable {
    type Target = HashMap<String, ServiceRoute>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ServiceRouteTable {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl ServiceRouteTable {
    /// Call the GraphQL query of the specified service.
    #[instrument(err(Debug), ret, level = "trace")]
    pub async fn query(
        &self,
        service: impl AsRef<str> + std::fmt::Debug,
        request: Request,
        header_map: Option<&HeaderMap>,
        introspection: Option<bool>,
    ) -> anyhow::Result<Response> {
        let service = service.as_ref();
        let route = self
            .0
            .get(service)
            .ok_or_else(|| anyhow::anyhow!("Service '{}' is not defined in the routing table.", service))?;

        let introspection = introspection.unwrap_or(false);

        let scheme = match route.tls {
            true => "https",
            false => "http",
        };

        let url = if introspection {
            match &route.introspection_path {
                Some(path) => format!("{}://{}{}", scheme, route.addr, path),
                None => format!("{}://{}", scheme, route.addr),
            }
        } else {
            match &route.query_path {
                Some(path) => format!("{}://{}{}", scheme, route.addr, path),
                None => format!("{}://{}", scheme, route.addr),
            }
        };

        let raw_resp = HTTP_CLIENT
            .post(&url)
            .headers(header_map.cloned().unwrap_or_default())
            .json(&request)
            .send()
            .await?;

        if !raw_resp.status().is_success() {
            let body = raw_resp.text().await?;
            return Err(anyhow::anyhow!(
                "received non-2xx response from service \"{}\", body: \"{}\"",
                service,
                body
            ));
        }

        let mut headers: HashMap<String, Vec<String>> = HashMap::new();

        for (key, val) in raw_resp.headers().iter() {
            match headers.get_mut(key.as_str()) {
                Some(x) => {
                    x.push(val.to_str().unwrap().to_string());
                },
                None => {
                    headers.insert(key.as_str().to_string(), vec![val.to_str().unwrap().to_string()]);
                },
            }
        }

        let mut resp = raw_resp.json::<Response>().await?;
        resp.headers = Some(headers);
        Ok(resp)
    }
}
