use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Args, Parser};
use graphgate_handler::{auth::AuthConfig, ServiceRoute, ServiceRouteTable};
use serde::Deserialize;
use tracing::instrument;

#[derive(Debug, Default, Deserialize, Parser)]
pub struct Config {
    /// Path of the config file
    #[clap(long, env = "CONFIG_FILE", default_value = "config.toml")]
    #[serde(skip)]
    pub file: PathBuf,

    #[clap(long, env, default_value = "127.0.0.1:8000")]
    #[serde(default = "default_bind")]
    pub bind: String,

    #[clap(long, env, default_value = "graphgate")]
    #[serde(default)]
    pub path: String,

    #[serde(default)]
    pub gateway_name: String,

    #[clap(long, env, value_delimiter = ',')]
    #[serde(default)]
    pub forward_headers: Vec<String>,

    #[clap(long, env, value_delimiter = ',')]
    #[serde(default)]
    pub receive_headers: Vec<String>,

    #[clap(flatten)]
    pub jaeger: Option<JaegerConfig>,

    #[clap(flatten)]
    pub cors: Option<CorsConfig>,

    #[clap(flatten)]
    pub authorization: Option<AuthConfig>,

    #[clap(skip)]
    #[serde(default)]
    pub services: Vec<ServiceConfig>,
}

#[derive(Args, Debug, Deserialize, Clone)]
pub struct ServiceConfig {
    pub name: String,
    pub addr: String,
    #[serde(default)]
    pub tls: bool,
    pub query_path: Option<String>,
    pub subscribe_path: Option<String>,
    pub introspection_path: Option<String>,
    pub websocket_path: Option<String>,
}

impl ServiceConfig {
    // websocket path should default to query path unless set
    fn default_or_set_websocket_path(&self) -> Option<String> {
        if self.websocket_path.is_some() {
            self.websocket_path.clone()
        } else {
            self.query_path.clone()
        }
    }
}

#[derive(Args, Clone, Debug, Deserialize)]
pub struct CorsConfig {
    #[clap(long, env = "CORS_ALLOW_METHODS", value_delimiter = ',')]
    pub allow_methods: Option<Vec<String>>,

    #[clap(long, env = "CORS_ALLOW_CREDENTIALS")]
    pub allow_credentials: Option<bool>,

    #[clap(long, env = "CORS_ALLOW_HEADERS", value_delimiter = ',')]
    pub allow_headers: Option<Vec<String>>,

    #[clap(long, env = "CORS_ALLOW_ORIGINS", value_delimiter = ',')]
    pub allow_origins: Option<Vec<String>>,
}

#[derive(Args, Clone, Debug, Deserialize)]
pub struct JaegerConfig {
    #[clap(long, env = "JAEGER_AGENT_ENDPOINT")]
    pub agent_endpoint: Option<String>,

    #[clap(long, env = "JAEGER_SERVICE_NAME", default_value = "graphgate")]
    #[serde(default = "default_jaeger_service_name")]
    pub service_name: String,
}

impl Config {
    /// Parse the config file and environment variables.
    /// If the config file exists, it will be parsed first and ignore
    /// environment variables.
    pub fn try_parse() -> anyhow::Result<Self> {
        let mut env_config = Config::parse();

        if Path::exists(&env_config.file) {
            let file_config = std::fs::read_to_string(&env_config.file).with_context(|| {
                format!(
                    "Failed to read config file '{}'.",
                    &env_config.file.display()
                )
            })?;
            let mut file_config: Config = toml::from_str(&file_config).with_context(|| {
                format!(
                    "Failed to parse config file '{}'.",
                    &env_config.file.display()
                )
            })?;

            // Override service URI with env var if set
            for service in &mut file_config.services {
                if let Ok(addr) = std::env::var(format!(
                    "SERVICE_{}_ADDR",
                    service.name.to_ascii_uppercase()
                )) {
                    tracing::info!("Overriding service '{}' addr with env var", service.name);
                    service.addr = addr;
                }
            }

            Ok(file_config)
        } else {
            let env_prefix = "SERVICE_";

            let mut service_prefixes = std::env::vars()
                .filter(|(name, _)| name.starts_with(env_prefix))
                .map(|(name, _)| name.trim_start_matches(env_prefix).to_string())
                .map(|name| name.split('_').next().unwrap().to_string())
                .collect::<Vec<String>>();

            service_prefixes.dedup();

            // Parse dynamically environment variables for services.
            // The following environment variables are parsed:
            // SERVICE_<SERVICE_NAME>_NAME
            // SERVICE_<SERVICE_NAME>_ADDR
            // SERVICE_<SERVICE_NAME>_TLS
            // SERVICE_<SERVICE_NAME>_QUERY_PATH
            // SERVICE_<SERVICE_NAME>_SUBSCRIBE_PATH
            // SERVICE_<SERVICE_NAME>_INTROSPECTION_PATH
            // SERVICE_<SERVICE_NAME>_WEBSOCKET_PATH
            env_config.services = service_prefixes
                .into_iter()
                .map(|service_prefix| ServiceConfig {
                    name: std::env::var(format!("{}{}_NAME", env_prefix, service_prefix))
                        .unwrap_or(service_prefix.to_ascii_lowercase()),
                    addr: std::env::var(format!("{}{}_ADDR", env_prefix, service_prefix))
                        .with_context(|| {
                            format!(
                                "Missing required environment variable '{}{}_ADDR'.",
                                env_prefix, service_prefix
                            )
                        })
                        .unwrap_or_default(),
                    tls: std::env::var(format!("{}{}_TLS", env_prefix, service_prefix))
                        .unwrap_or("false".to_string())
                        .parse()
                        .unwrap_or_default(),
                    query_path: if let Ok(path) =
                        std::env::var(format!("{}{}_QUERY_PATH", env_prefix, service_prefix))
                    {
                        Some(path)
                    } else {
                        None
                    },
                    subscribe_path: if let Ok(path) =
                        std::env::var(format!("{}{}_SUBSCRIBE_PATH", env_prefix, service_prefix))
                    {
                        Some(path)
                    } else {
                        None
                    },
                    introspection_path: if let Ok(path) = std::env::var(format!(
                        "{}{}_INTROSPECTION_PATH",
                        env_prefix, service_prefix
                    )) {
                        Some(path)
                    } else {
                        None
                    },
                    websocket_path: if let Ok(path) =
                        std::env::var(format!("{}{}_WEBSOCKET_PATH", env_prefix, service_prefix))
                    {
                        Some(path)
                    } else {
                        None
                    },
                })
                .collect::<Vec<ServiceConfig>>();

            Ok(env_config)
        }
    }

    #[instrument(ret, level = "trace")]
    pub fn create_route_table(&self) -> ServiceRouteTable {
        let mut route_table = ServiceRouteTable::default();
        for service in &self.services {
            route_table.insert(service.name.clone(), ServiceRoute {
                addr: service.addr.clone(),
                tls: service.tls,
                query_path: service.query_path.clone(),
                subscribe_path: service.subscribe_path.clone(),
                introspection_path: service.introspection_path.clone(),
                websocket_path: service.default_or_set_websocket_path(),
            });
        }
        route_table
    }
}

fn default_bind() -> String {
    "127.0.0.1:8000".to_string()
}

fn default_jaeger_service_name() -> String {
    "graphgate".to_string()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use serial_test::serial;
    use tempfile::NamedTempFile;

    use super::*;

    #[tokio::test]
    #[serial]
    async fn parse_base_env_vars() {
        std::env::set_var("CONFIG_FILE", "does_not_exist.toml");

        std::env::set_var("FORWARD_HEADERS", "authorization,x-test");

        let parsed_config = Config::try_parse().expect("Failed to parse config");
        assert_eq!(parsed_config.bind, "127.0.0.1:8000");
        assert_eq!(
            parsed_config.file.display().to_string(),
            "does_not_exist.toml"
        );
        assert_eq!(parsed_config.gateway_name, "graphgate");
        assert_eq!(
            parsed_config.forward_headers,
            vec!["authorization".to_string(), "x-test".to_string()]
        );
    }

    #[tokio::test]
    #[serial]
    async fn parse_auth_env_vars() {
        std::env::set_var("CONFIG_FILE", "does_not_exist.toml");

        std::env::set_var("AUTH_ENABLED", "true");
        std::env::set_var("AUTH_HEADER_NAME", "X-Authorization");
        std::env::set_var("AUTH_HEADER_PREFIX", "");
        std::env::set_var("AUTH_REQUIRED", "true");
        std::env::set_var("AUTH_JWKS", "https://test.tld/jwks.json");

        let parsed_config = Config::try_parse().expect("Failed to parse config");
        let auth_config = parsed_config.authorization.expect("No auth config");
        assert!(auth_config.enabled);
        assert_eq!(auth_config.header_name, "X-Authorization");
        assert_eq!(auth_config.header_prefix, "");
        assert!(auth_config.required);
        assert_eq!(auth_config.jwks, "https://test.tld/jwks.json");
    }

    #[tokio::test]
    #[serial]
    async fn parse_service_env_vars() {
        std::env::set_var("CONFIG_FILE", "does_not_exist.toml");

        std::env::set_var("SERVICE_TESTENV_NAME", "testenv");
        std::env::set_var("SERVICE_TESTENV_ADDR", "http://test.tld");
        std::env::set_var("SERVICE_TESTENV_TLS", "true");
        std::env::set_var("SERVICE_TESTENV_QUERY_PATH", "/graphql");
        std::env::set_var("SERVICE_TESTENV_SUBSCRIBE_PATH", "/graphql/subscribe");
        std::env::set_var(
            "SERVICE_TESTENV_INTROSPECTION_PATH",
            "/graphql/introspection",
        );
        std::env::set_var("SERVICE_TESTENV_WEBSOCKET_PATH", "/graphql/ws");

        let parsed_config = Config::try_parse().expect("Failed to parse config");
        let service_config = parsed_config.services.first().expect("No service config");
        assert_eq!(service_config.name, "testenv");
        assert_eq!(service_config.addr, "http://test.tld");
        assert!(service_config.tls);
        assert_eq!(service_config.query_path, Some("/graphql".to_string()));
        assert_eq!(
            service_config.subscribe_path,
            Some("/graphql/subscribe".to_string())
        );
        assert_eq!(
            service_config.introspection_path,
            Some("/graphql/introspection".to_string())
        );
        assert_eq!(
            service_config.websocket_path,
            Some("/graphql/ws".to_string())
        );
    }

    #[tokio::test]
    #[serial]
    async fn parse_config_file() {
        let mut tmpfile =
            NamedTempFile::with_prefix("graphgate").expect("Failed to create temp config");
        write!(
            tmpfile,
            r#"
        bind = "0.0.0.0:4000"
        forward_headers = ["authorization"]
        [[services]]
        name = "test"
        addr = "test:4000"
        query_path = "/public/graphql"
        subscribe_path = "/public/graphql"
        introspection_path = "/public/graphql"
        websocket_path = "/public/graphql"
        "#
        )
        .expect("Failed to write temp config");
        std::env::set_var("CONFIG_FILE", tmpfile.path().display().to_string());
        std::env::set_var("BIND", "127.0.0.1:8000");

        let parsed_config = Config::try_parse().expect("Failed to parse config");
        assert_eq!(parsed_config.bind, "0.0.0.0:4000");
        assert_eq!(
            parsed_config.forward_headers,
            vec!["authorization".to_string()]
        );
        assert_eq!(parsed_config.services.len(), 1);

        let service_config = parsed_config.services.first().expect("No service config");
        assert_eq!(service_config.name, "test");
        assert_eq!(service_config.addr, "test:4000");
        assert_eq!(
            service_config.query_path,
            Some("/public/graphql".to_string())
        );
        assert_eq!(
            service_config.subscribe_path,
            Some("/public/graphql".to_string())
        );
        assert_eq!(
            service_config.introspection_path,
            Some("/public/graphql".to_string())
        );
        assert_eq!(
            service_config.websocket_path,
            Some("/public/graphql".to_string())
        );

        std::env::remove_var("CONFIG_FILE");
        std::env::remove_var("BIND");
    }

    #[tokio::test]
    #[serial]
    async fn parse_config_file_no_auth() {
        let mut tmpfile =
            NamedTempFile::with_prefix("graphgate").expect("Failed to create temp config");
        write!(
            tmpfile,
            r#"
        [[services]]
        name = "testoverride"
        addr = "test:4000"
        "#
        )
        .expect("Failed to write temp config");
        std::env::set_var("CONFIG_FILE", tmpfile.path().display().to_string());

        let parsed_config = Config::try_parse().expect("Failed to parse config");
        assert!(parsed_config.authorization.is_none());

        std::env::remove_var("CONFIG_FILE");
    }

    #[tokio::test]
    #[serial]
    async fn parse_config_file_service_addr_override() {
        let mut tmpfile =
            NamedTempFile::with_prefix("graphgate").expect("Failed to create temp config");
        write!(
            tmpfile,
            r#"
        [[services]]
        name = "testoverride"
        addr = "test:4000"
        "#
        )
        .expect("Failed to write temp config");
        std::env::set_var("CONFIG_FILE", tmpfile.path().display().to_string());
        std::env::set_var("SERVICE_TESTOVERRIDE_ADDR", "127.0.0.1:8000");

        let parsed_config = Config::try_parse().expect("Failed to parse config");
        assert_eq!(parsed_config.services.len(), 1);

        let service_config = parsed_config.services.first().expect("No service config");
        assert_eq!(service_config.name, "testoverride");
        assert_eq!(service_config.addr, "127.0.0.1:8000");

        std::env::remove_var("CONFIG_FILE");
        std::env::remove_var("SERVICE_TESTOVERRIDE_ADDR");
    }
}
