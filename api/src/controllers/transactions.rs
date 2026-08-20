use crate::error::ApiError;
use crate::infra::state::{AppRouter, AppTransactionService};
use axum::Json;
use axum::extract::{Query, State};
use axum::routing::get;
use serde::Deserialize;
use shared::api::TransactionLookup;
use std::collections::HashSet;

/// Clusters cap at 64 transactions; 128 leaves headroom without inviting abuse.
const MAX_TXIDS: usize = 128;

pub trait TransactionsControllerRouter {
    fn add_transaction_routes(self) -> Self;
}

impl TransactionsControllerRouter for AppRouter {
    fn add_transaction_routes(self) -> Self {
        self.route("/transactions", get(transactions))
    }
}

#[derive(Debug, Deserialize)]
struct TxidsQuery {
    // Comma-separated list of txids to look up.
    txids: String,
}

/// Looks up transactions by txid.
async fn transactions(
    State(service): State<AppTransactionService>,
    Query(query): Query<TxidsQuery>,
) -> Result<Json<TransactionLookup>, ApiError> {
    let txids = parse_txids(&query.txids)?;
    let lookup = service.lookup(&txids).await?;
    Ok(Json(lookup))
}

fn parse_txids(raw: &str) -> Result<Vec<String>, ApiError> {
    if raw.is_empty() {
        return Err(ApiError::BadRequest(
            "`txids` must not be empty".to_string(),
        ));
    }

    let segments: Vec<&str> = raw.split(',').collect();
    if segments.len() > MAX_TXIDS {
        return Err(ApiError::BadRequest(format!(
            "at most {MAX_TXIDS} txids allowed, got {}",
            segments.len()
        )));
    }

    for segment in &segments {
        if !is_valid_txid(segment) {
            return Err(ApiError::BadRequest(format!("invalid txid: `{segment}`")));
        }
    }

    let mut seen = HashSet::with_capacity(segments.len());
    let mut txids = Vec::with_capacity(segments.len());
    for segment in segments {
        if seen.insert(segment) {
            txids.push(segment.to_string());
        }
    }
    Ok(txids)
}

/// 64-char lowercase hex, matching how a txid is stored and displayed.
fn is_valid_txid(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_rejected() {
        assert!(matches!(parse_txids(""), Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn too_many_segments_is_rejected_before_dedupe() {
        let raw = vec!["a".repeat(64); 129].join(",");
        assert!(matches!(parse_txids(&raw), Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn a_63_char_txid_is_rejected() {
        let raw = "a".repeat(63);
        assert!(matches!(parse_txids(&raw), Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn a_65_char_txid_is_rejected() {
        let raw = "a".repeat(65);
        assert!(matches!(parse_txids(&raw), Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn an_uppercase_txid_is_rejected() {
        let raw = "A".repeat(64);
        assert!(matches!(parse_txids(&raw), Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn a_non_hex_txid_is_rejected() {
        let raw = "g".repeat(64);
        assert!(matches!(parse_txids(&raw), Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn a_valid_pair_is_accepted() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let raw = format!("{a},{b}");
        assert_eq!(parse_txids(&raw).unwrap(), vec![a, b]);
    }

    #[test]
    fn dedupe_preserves_first_seen_order() {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        let raw = format!("{a},{b},{a}");
        assert_eq!(parse_txids(&raw).unwrap(), vec![a, b]);
    }
}
