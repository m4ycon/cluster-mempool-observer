use crate::db::SystemEventRepository;
use crate::db::models::{NewSystemEvent, SystemEventKind};
use crate::error::ApiError;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct SystemEventService {
    system_event_repository: SystemEventRepository,
}

impl SystemEventService {
    pub fn new(system_event_repository: SystemEventRepository) -> Self {
        Self {
            system_event_repository,
        }
    }

    /// Lifecycle events in `[from, to]`, oldest first.
    pub async fn list(
        &self,
        from: Option<OffsetDateTime>,
        to: Option<OffsetDateTime>,
    ) -> Result<Vec<shared::events::SystemEvent>, ApiError> {
        let rows = self.system_event_repository.list(from, to).await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Records a lifecycle event; failures are logged and swallowed, never propagated.
    pub async fn record(&self, kind: SystemEventKind, details: serde_json::Value) {
        let event = NewSystemEvent { kind, details };
        if let Err(e) = self.system_event_repository.insert(&event).await {
            tracing::warn!("system_event: failed to record {kind:?}: {e}");
        }
    }

    /// Most recent event of `kind`, if any.
    pub async fn latest_of_kind(
        &self,
        kind: SystemEventKind,
    ) -> Result<Option<shared::events::SystemEvent>, ApiError> {
        let row = self.system_event_repository.latest_of_kind(kind).await?;
        Ok(row.map(Into::into))
    }
}
