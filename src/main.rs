#![forbid(unsafe_code)]

mod config;
mod k8s;
mod options;

use std::{convert::Infallible, net::SocketAddr, sync::Arc};

use anyhow::{Context, Result};
use clap::Parser;
use config::Config;
use futures_util::FutureExt;
use graphgate_handler::{
    auth::{Auth, AuthError},
    handler,
    handler::HandlerConfig,
    SharedRouteTable,
};
use graphgate_planner::{Response, ServerError};
use opentelemetry::{
    global, global::GlobalTracerProvider, sdk::metrics::MeterProvider,
    trace::noop::NoopTracerProvider,
};
use options::Options;
use prometheus::{Encoder, Registry, TextEncoder};
use tokio::{signal, time::Duration};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use value::ConstValue;
use warp::{http::Response as HttpResponse, hyper::StatusCode, Filter, Rejection, Reply};

fn init_tracing() {
    tracing_subscriber::registry()
        .with(fmt::layer().compact().with_target(false))
        .with(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("info"))
                .unwrap(),
        )
        .init();
}

async fn update_route_table_in_k8s(shared_route_table: SharedRouteTable, gateway_name: String) {
    let mut prev_route_table = None;
    loop {
        match k8s::find_graphql_services(&gateway_name).await {
            Ok(route_table) => {
                if Some(&route_table) != prev_route_table.as_ref() {
                    tracing::info!(route_table = ?route_table, "Route table updated.");
                    shared_route_table.set_route_table(route_table.clone());
                    prev_route_table = Some(route_table);
                }
            }
            Err(err) => {
                tracing::error!(error = %err, "Failed to find graphql services.");
            }
        }

        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

fn init_tracer(config: &Config) -> Result<GlobalTracerProvider> {
    let uninstall = match &config.jaeger {
        Some(config) => {
            tracing::info!(
                agent_endpoint = %config.agent_endpoint,
                service_name = %config.service_name,
                "Initialize Jaeger"
            );
            let provider = opentelemetry_jaeger::new_agent_pipeline()
                .with_endpoint(&config.agent_endpoint)
                .with_service_name(&config.service_name)
                .build_batch(opentelemetry::runtime::Tokio)
                .context("Failed to initialize jaeger.")?;
            global::set_tracer_provider(provider)
        }
        None => {
            let provider = NoopTracerProvider::new();
            global::set_tracer_provider(provider)
        }
    };
    Ok(uninstall)
}

pub fn metrics(
    registry: Registry,
) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
    warp::path!("metrics").and(warp::get()).map({
        move || {
            let mut buffer = Vec::new();
            let encoder = TextEncoder::new();
            let metric_families = registry.gather();
            if let Err(err) = encoder.encode(&metric_families, &mut buffer) {
                return HttpResponse::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(err.to_string().into_bytes())
                    .unwrap();
            }
            HttpResponse::builder()
                .status(StatusCode::OK)
                .body(buffer)
                .unwrap()
        }
    })
}

async fn handle_rejection(err: Rejection) -> std::result::Result<impl Reply, Infallible> {
    let (code, message) = if err.is_not_found() {
        (StatusCode::OK, "Not Found".to_string())
    } else if let Some(e) = err.find::<AuthError>() {
        (StatusCode::OK, e.to_string())
    } else {
        tracing::error!("unhandled error: {:?}", err);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error".to_string(),
        )
    };

    let res = warp::reply::json(&Response {
        data: ConstValue::Null,
        errors: vec![ServerError::new(message)],
        extensions: Default::default(),
        headers: None,
    });

    Ok(warp::reply::with_status(res, code))
}

#[tokio::main]
async fn main() -> Result<()> {
    let options: Options = Options::parse();
    init_tracing();

    let config = toml::from_str::<Config>(
        &std::fs::read_to_string(&options.config)
            .with_context(|| format!("Failed to load config file '{}'.", options.config))?,
    )
    .with_context(|| format!("Failed to parse config file '{}'.", options.config))?;
    let _uninstall = init_tracer(&config)?;
    let registry = Registry::new();
    let exporter = opentelemetry_prometheus::exporter()
        .with_registry(registry.clone())
        .build()?;
    let meter_provider = MeterProvider::builder().with_reader(exporter).build();
    global::set_meter_provider(meter_provider);

    let mut shared_route_table = SharedRouteTable::default();
    if !config.services.is_empty() {
        tracing::info!("Route table in the configuration file.");
        shared_route_table.set_route_table(config.create_route_table());
        shared_route_table.set_receive_headers(config.receive_headers);
    } else if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() {
        tracing::info!("Route table within the current namespace in Kubernetes cluster.");
        shared_route_table.set_receive_headers(config.receive_headers);
        tokio::spawn(update_route_table_in_k8s(
            shared_route_table.clone(),
            config.gateway_name.clone(),
        ));
    } else {
        tracing::info!("Route table is empty.");
        return Ok(());
    }

    let handler_config = HandlerConfig {
        shared_route_table,
        forward_headers: Arc::new(config.forward_headers),
    };

    let auth: Arc<Auth> = match config.authorization {
        Some(config) => Arc::new(Auth::try_new(config).await?),
        None => Arc::new(Auth::default()),
    };

    let cors = config.cors.map(|cors_config| {
        warp::cors()
            .allow_any_origin()
            .allow_methods(
                cors_config
                    .allow_methods
                    .unwrap_or_default()
                    .iter()
                    .map(|s| s as &str)
                    .collect::<Vec<&str>>(),
            )
            .allow_credentials(cors_config.allow_credentials.unwrap_or(false))
            .allow_headers(cors_config.allow_headers.unwrap_or_default())
            .allow_origins(
                cors_config
                    .allow_origins
                    .unwrap_or_default()
                    .iter()
                    .map(|s| s as &str)
                    .collect::<Vec<&str>>(),
            )
    });

    let graphql = warp::path::end().and(
        handler::graphql_request(auth.clone(), handler_config.clone())
            .or(handler::graphql_websocket(auth, handler_config.clone()))
            .or(handler::graphql_playground()),
    );
    let health = warp::path!("health").map(|| warp::reply::json(&"healthy"));

    let bind_addr: SocketAddr = config
        .bind
        .parse()
        .context(format!("Failed to parse bind addr '{}'", config.bind))?;
    if let Some(warp_cors) = cors {
        let routes = graphql.or(health).or(metrics(registry)).with(warp_cors);
        let (addr, server) = warp::serve(routes.recover(handle_rejection))
            .bind_with_graceful_shutdown(bind_addr, signal::ctrl_c().map(|_| ()));
        tracing::info!(addr = %addr, "Listening");
        server.await;
        tracing::info!("Server shutdown");
    } else {
        let routes = graphql.or(health).or(metrics(registry));
        let (addr, server) = warp::serve(routes.recover(handle_rejection))
            .bind_with_graceful_shutdown(bind_addr, signal::ctrl_c().map(|_| ()));
        tracing::info!(addr = %addr, "Listening");
        server.await;
        tracing::info!("Server shutdown");
    }

    Ok(())
}
