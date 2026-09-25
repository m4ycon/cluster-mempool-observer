use crate::infra::state::AppState;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use shared::models::GetBlockchainInfoModel;
use std::sync::{Arc, RwLock};

/// How far startup has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Node unreachable, still in initial block download, or still loading its mempool.
    WaitingForNode,
    /// Node is good; reconciling mempool and blocks.
    Bootstrapping,
    /// Bootstrap finished, watchers running.
    Ready,
}

/// What the node last told us, or why it did not.
#[derive(Debug, Clone, Default, Serialize)]
pub struct NodeReport {
    pub reachable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_progress: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initial_block_download: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mempool_loaded: Option<bool>,
}

/// Startup progress, shared between the bootstrap task and the http handlers.
#[derive(Clone)]
pub struct Readiness {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Debug, Clone)]
struct Inner {
    phase: Phase,
    node: NodeReport,
}

impl Default for Readiness {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                phase: Phase::WaitingForNode,
                node: NodeReport::default(),
            })),
        }
    }
}

impl Readiness {
    /// Records a successful `getblockchaininfo`.
    pub fn record_node(&self, info: &GetBlockchainInfoModel) {
        self.write(|inner| {
            inner.node = NodeReport {
                reachable: true,
                error: None,
                blocks: Some(info.blocks),
                headers: Some(info.headers),
                verification_progress: Some(info.verification_progress),
                initial_block_download: Some(info.initial_block_download),
                mempool_loaded: None,
            };
        });
    }

    /// Records a successful `getmempoolinfo`, on top of the last `record_node`.
    pub fn record_mempool_loaded(&self, loaded: bool) {
        self.write(|inner| inner.node.mempool_loaded = Some(loaded));
    }

    /// Records a node that could not be reached.
    pub fn record_node_error(&self, error: impl std::fmt::Display) {
        self.write(|inner| {
            inner.node = NodeReport {
                reachable: false,
                error: Some(error.to_string()),
                ..NodeReport::default()
            };
        });
    }

    pub fn set_phase(&self, phase: Phase) {
        self.write(|inner| inner.phase = phase);
    }

    pub fn is_ready(&self) -> bool {
        self.read(|inner| inner.phase) == Phase::Ready
    }

    pub fn report(&self) -> HealthReport {
        let inner = self.read(|inner| inner.clone());
        HealthReport {
            ready: inner.phase == Phase::Ready,
            phase: inner.phase,
            node: inner.node,
        }
    }

    fn write(&self, f: impl FnOnce(&mut Inner)) {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        f(&mut guard);
    }

    fn read<T>(&self, f: impl FnOnce(&Inner) -> T) -> T {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        f(&guard)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    pub ready: bool,
    pub phase: Phase,
    pub node: NodeReport,
}

/// Middleware that rejects data routes until bootstrap has finished.
pub async fn require_ready(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    if state.readiness.is_ready() {
        return next.run(request).await;
    }

    (
        StatusCode::SERVICE_UNAVAILABLE,
        [("retry-after", "10")],
        axum::Json(state.readiness.report()),
    )
        .into_response()
}
