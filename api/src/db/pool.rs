use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub type DbPool = Pool<AsyncPgConnection>;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub fn run_migrations(url: &str) -> Result<(), BoxError> {
    let mut conn = PgConnection::establish(url)?;
    conn.run_pending_migrations(MIGRATIONS)?;
    Ok(())
}

pub fn build_pool(url: &str) -> Result<DbPool, BoxError> {
    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(url);
    let pool = Pool::builder(config).build()?;
    Ok(pool)
}

pub fn build_pool_with_max_size(url: &str, max_size: usize) -> Result<DbPool, BoxError> {
    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(url);
    let pool = Pool::builder(config).max_size(max_size).build()?;
    Ok(pool)
}
