use crate::db::TransactionRepository;
use crate::db::models::Transaction;
use crate::error::ApiError;
use shared::api::{TransactionLookup, TransactionRef};
use std::collections::HashMap;

#[derive(Clone)]
pub struct TransactionService {
    transaction_repository: TransactionRepository,
}

impl TransactionService {
    pub fn new(transaction_repository: TransactionRepository) -> Self {
        Self {
            transaction_repository,
        }
    }

    /// Splits `txids` into `found` (in request order) and `missing`.
    pub async fn lookup(&self, txids: &[String]) -> Result<TransactionLookup, ApiError> {
        let rows = self.transaction_repository.find_by_txids(txids).await?;
        let mut by_txid: HashMap<String, Transaction> = rows
            .into_iter()
            .map(|row| (row.txid.clone(), row))
            .collect();

        let mut found = Vec::new();
        let mut missing = Vec::new();
        for txid in txids {
            match by_txid.remove(txid) {
                Some(row) => found.push(TransactionRef::from(row)),
                None => missing.push(txid.clone()),
            }
        }

        Ok(TransactionLookup { found, missing })
    }
}
