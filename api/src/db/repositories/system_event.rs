use super::RepoResult;
use crate::db::instrument::query;
use crate::db::models::{NewSystemEvent, SystemEventKind, SystemEventRow};
use crate::db::pool::DbPool;
use crate::db::schema::system_events;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use time::OffsetDateTime;

const REPO_LABEL: &str = "system_event";

#[derive(Clone)]
pub struct SystemEventRepository {
    pool: DbPool,
}

impl SystemEventRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    pub async fn insert(&self, event: &NewSystemEvent) -> RepoResult<usize> {
        query(&self.pool, REPO_LABEL, "insert", async |conn| {
            diesel::insert_into(system_events::table)
                .values(event)
                .execute(conn)
                .await
        })
        .await
    }

    /// Most recent row of `kind`, if any.
    pub async fn latest_of_kind(
        &self,
        kind: SystemEventKind,
    ) -> RepoResult<Option<SystemEventRow>> {
        query(&self.pool, REPO_LABEL, "latest_of_kind", async |conn| {
            system_events::table
                .filter(system_events::kind.eq(kind))
                .order(system_events::id.desc())
                .select(SystemEventRow::as_select())
                .first(conn)
                .await
                .optional()
        })
        .await
    }

    /// Oldest first. `from`/`to` bound `created_at` inclusively; both are optional
    /// and an omitted bound is left open.
    pub async fn list(
        &self,
        from: Option<OffsetDateTime>,
        to: Option<OffsetDateTime>,
    ) -> RepoResult<Vec<SystemEventRow>> {
        query(&self.pool, REPO_LABEL, "list", async |conn| {
            let mut q = system_events::table.into_boxed();
            if let Some(from) = from {
                q = q.filter(system_events::created_at.ge(from));
            }
            if let Some(to) = to {
                q = q.filter(system_events::created_at.le(to));
            }
            q.order(system_events::id.asc())
                .select(SystemEventRow::as_select())
                .load(conn)
                .await
        })
        .await
    }
}
