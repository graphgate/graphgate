use std::{collections::HashMap, convert::Infallible, sync::Arc};

use anyhow::Context;
use http::{header::AUTHORIZATION, HeaderMap};
use jsonwebtoken::{jwk::JwkSet, DecodingKey};
use serde::Deserialize;
use thiserror::Error;
use warp::{header::headers_cloned, Filter, Rejection};

#[derive(Default)]
pub struct Auth {
    pub config: AuthConfig,
    pub decoding_keys: HashMap<String, DecodingKey>,
}

#[derive(Debug, Default, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_header_name")]
    pub header_name: String,
    #[serde(default)]
    pub header_prefix: String,
    #[serde(default)]
    pub require_auth: bool,

    pub jwks: String,
}

impl Auth {
    pub async fn try_new(config: AuthConfig) -> anyhow::Result<Self> {
        let jwks = reqwest::get(&config.jwks)
            .await
            .context("failed to fetch jwks")?
            .json::<JwkSet>()
            .await
            .context("failed to decode jwks")?;

        let decoding_keys = jwks
            .keys
            .into_iter()
            .filter_map(|jwk| {
                let res =
                    DecodingKey::from_jwk(&jwk).context("failed to create decoding key from jwk");
                jwk.common.key_id.map(|kid| res.map(|key| (kid, key)))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;

        Ok(Self {
            config,
            decoding_keys,
        })
    }
}

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("missing authorization header")]
    MissingAuthorizationHeader,

    #[error("authorization prefix not found")]
    AuthorizationPrefixNotFound,

    #[error("jwt decoding error: {0}")]
    DecodingError(#[from] jsonwebtoken::errors::Error),

    #[error("missing kid in authorization header")]
    MissingKid,

    #[error("invalid kid in authorization header")]
    InvalidKid,
}

impl warp::reject::Reject for AuthError {}

pub fn with_auth_state(
    auth: Arc<Auth>,
) -> impl Filter<Extract = (Arc<Auth>,), Error = Infallible> + Clone {
    warp::any().map(move || auth.clone())
}

pub fn with_auth(auth: Arc<Auth>) -> impl Filter<Extract = ((),), Error = Rejection> + Clone {
    headers_cloned()
        .and(with_auth_state(auth))
        .and_then(jwt_auth_validate)
}

async fn jwt_auth_validate(header_map: HeaderMap, auth: Arc<Auth>) -> Result<(), Rejection> {
    if !auth.config.enabled {
        return Ok(());
    }

    let header = header_map.get(auth.config.header_name.as_str());
    if header.is_none() && auth.config.require_auth {
        return Err(warp::reject::custom(AuthError::MissingAuthorizationHeader));
    }

    if let Some(header) = header {
        let token = header
            .to_str()
            .unwrap_or_default()
            .strip_prefix(&auth.config.header_prefix)
            .ok_or(warp::reject::custom(AuthError::AuthorizationPrefixNotFound))?
            .trim_start();

        let token_header = jsonwebtoken::decode_header(token).map_err(AuthError::DecodingError)?;

        let kid = token_header.kid.ok_or_else(|| AuthError::MissingKid)?;

        let decoding_key = auth
            .decoding_keys
            .get(&kid)
            .ok_or_else(|| AuthError::InvalidKid)?;

        jsonwebtoken::decode::<serde_json::Value>(
            token,
            decoding_key,
            &jsonwebtoken::Validation::new(token_header.alg),
        )
        .map_err(AuthError::DecodingError)?;
    }

    Ok(())
}

fn default_header_name() -> String {
    AUTHORIZATION.to_string()
}
