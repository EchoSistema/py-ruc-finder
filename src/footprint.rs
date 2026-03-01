use std::future::{Ready, ready};
use std::sync::Arc;

use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready};
use log::warn;
use serde::Serialize;
use sha2::{Digest, Sha256};

const FOOTPRINT_PATH: &str = "/api/v1/footprints";

/// Fire-and-forget tracking client for the EchoSistema Footprint API.
pub struct FootprintService {
    client: reqwest::Client,
    api_url: String,
    public_key: String,
}

#[derive(Serialize)]
struct FootprintPayload {
    session_id: String,
    event: &'static str,
    page_title: &'static str,
    page_url: String,
    event_time: String,
    referer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url_params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<String>,
}

impl FootprintService {
    pub fn new(api_base_url: String, public_key: String) -> Self {
        let api_url = format!("{}{}", api_base_url.trim_end_matches('/'), FOOTPRINT_PATH);
        let client = reqwest::Client::builder()
            .user_agent("ruc_finder/0.1")
            .build()
            .expect("Failed to build HTTP client");
        Self {
            client,
            api_url,
            public_key,
        }
    }

    /// Send a tracking event (fire-and-forget via tokio::spawn).
    pub fn track(
        self: &Arc<Self>,
        session_id: String,
        event: &'static str,
        page_title: &'static str,
        page_url: String,
        referer: String,
        url_params: Option<serde_json::Value>,
        language: Option<String>,
    ) {
        let this = Arc::clone(self);
        let payload = FootprintPayload {
            session_id,
            event,
            page_title,
            page_url,
            referer,
            event_time: chrono::Utc::now().to_rfc3339(),
            url_params,
            language,
        };

        tokio::spawn(async move {
            match this
                .client
                .post(&this.api_url)
                .header("X-PUBLIC-KEY", &this.public_key)
                .json(&payload)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        warn!("Footprint API returned {status}: {body}");
                    }
                }
                Err(e) => warn!("Footprint tracking failed: {e}"),
            }
        });
    }
}

/// Extract the primary language tag from an Accept-Language header.
/// e.g. "es-PY,es;q=0.9,en;q=0.8" → "es-PY"
/// Falls back to "es-PY" if empty.
fn parse_primary_language(header: &str) -> String {
    header
        .split(',')
        .next()
        .and_then(|s| s.split(';').next())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("es-PY")
        .to_string()
}

/// Derive a stable session_id from the client IP via SHA-256 truncation.
fn session_id_from_ip(ip: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ip.as_bytes());
    let hash = hasher.finalize();
    // First 16 hex chars (8 bytes) — enough to group by visitor
    hash[..8]
        .iter()
        .fold(String::with_capacity(16), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// Resolve event type and url_params from the request path and query string.
fn resolve_event(path: &str, query: &str) -> Option<(&'static str, &'static str, String, Option<serde_json::Value>)> {
    // Swagger UI
    if path.starts_with("/swagger-ui") {
        return Some(("page_view", "Swagger Ui", "/swagger-ui/".to_string(), None));
    }
    // OpenAPI JSON
    if path == "/api-docs/openapi.json" {
        return Some(("page_view", "Api Docs Openapi", "/api-docs/openapi.json".to_string(), None));
    }

    // API endpoints under /api/v1/ruc
    if !path.starts_with("/api/v1/ruc") {
        return None;
    }

    // GET /api/v1/ruc/search — fuzzy search
    if path == "/api/v1/ruc/search" {
        let params = query_string_to_value(query);
        return Some(("view_search_results", "Ruc Search", path.to_string(), Some(params)));
    }

    // GET /api/v1/ruc/{ruc}/dv
    if let Some(ruc) = path.strip_prefix("/api/v1/ruc/").and_then(|rest| rest.strip_suffix("/dv"))
        && !ruc.contains('/')
    {
        let params = serde_json::json!({ "ruc": ruc });
        return Some(("page_view", "Ruc Dv", path.to_string(), Some(params)));
    }

    // GET /api/v1/ruc/{ruc}/validate/{dv}
    if let Some(rest) = path.strip_prefix("/api/v1/ruc/")
        && let Some((ruc, tail)) = rest.split_once("/validate")
        && !ruc.contains('/')
    {
        let dv = tail.strip_prefix('/').unwrap_or("");
        let mut params = serde_json::json!({ "ruc": ruc });
        let title = if dv.is_empty() { "Ruc Dv Validate" } else { "Ruc Validate Dv" };
        if !dv.is_empty() {
            params["dv"] = serde_json::Value::String(dv.to_string());
        }
        return Some(("page_view", title, path.to_string(), Some(params)));
    }

    // GET /api/v1/ruc/{ruc} — single RUC lookup
    if let Some(ruc) = path.strip_prefix("/api/v1/ruc/")
        && !ruc.is_empty()
        && !ruc.contains('/')
    {
        let params = serde_json::json!({ "ruc": ruc });
        return Some(("page_view", "Ruc Number", path.to_string(), Some(params)));
    }

    // GET /api/v1/ruc — search with filters
    if path == "/api/v1/ruc" {
        let params = query_string_to_value(query);
        return Some(("view_search_results", "Ruc", path.to_string(), Some(params)));
    }

    None
}

fn query_string_to_value(query: &str) -> serde_json::Value {
    if query.is_empty() {
        return serde_json::json!({});
    }
    let map: serde_json::Map<String, serde_json::Value> = query
        .split('&')
        .filter_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            Some((k.to_string(), serde_json::Value::String(v.to_string())))
        })
        .collect();
    serde_json::Value::Object(map)
}

// ---------------------------------------------------------------------------
// Actix middleware
// ---------------------------------------------------------------------------

/// Middleware factory that intercepts requests and sends footprint events.
/// Holds an optional `Arc<FootprintService>` — when `None`, the middleware is a no-op.
pub struct FootprintMiddleware {
    pub service: Option<Arc<FootprintService>>,
}

impl<S, B> Transform<S, ServiceRequest> for FootprintMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type InitError = ();
    type Transform = FootprintMiddlewareService<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(FootprintMiddlewareService {
            service,
            footprint: self.service.clone(),
        }))
    }
}

pub struct FootprintMiddlewareService<S> {
    service: S,
    footprint: Option<Arc<FootprintService>>,
}

impl<S, B> Service<ServiceRequest> for FootprintMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = actix_web::Error;
    type Future = S::Future;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        if let Some(ref svc) = self.footprint {
            let path = req.path().to_string();
            let query = req.query_string().to_string();
            let ip = req
                .peer_addr()
                .map(|addr| addr.ip().to_string())
                .unwrap_or_default();

            if let Some((event, page_title, page_url, url_params)) = resolve_event(&path, &query) {
                let session_id = session_id_from_ip(&ip);
                let referer = if query.is_empty() {
                    path.clone()
                } else {
                    format!("{path}?{query}")
                };
                let language = req
                    .headers()
                    .get("Accept-Language")
                    .and_then(|v| v.to_str().ok())
                    .map(|s| parse_primary_language(s))
                    .unwrap_or_else(|| "es-PY".to_string());
                svc.track(session_id, event, page_title, page_url, referer, url_params, Some(language));
            }
        }

        self.service.call(req)
    }
}
