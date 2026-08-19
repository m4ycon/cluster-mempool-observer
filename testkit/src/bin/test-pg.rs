#![cfg_attr(feature = "strict", deny(warnings))]

//! Prepares the Postgres shared by a `cargo nextest` run: one migrated template
//! database, cloned into one database per test slot.
//!
//! Usage: `test-pg <base-url> <slots>`, where `<base-url>` carries everything up
//! to but not including the database name.

use diesel::prelude::*;
use diesel::sql_query;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

const TEMPLATE: &str = "mempool_test_template";

fn main() -> Result<(), BoxError> {
    let mut args = std::env::args().skip(1);
    let base = args.next().ok_or("usage: test-pg <base-url> <slots>")?;
    let slots: usize = args
        .next()
        .ok_or("usage: test-pg <base-url> <slots>")?
        .parse()?;

    let mut admin = PgConnection::establish(&format!("{base}/postgres"))?;

    // Rebuilt from scratch every run, so a new migration can never leave the
    // slot databases on a stale schema.
    recreate(&mut admin, TEMPLATE, None)?;
    api::db::run_migrations(&format!("{base}/{TEMPLATE}"))?;

    for slot in 0..slots {
        recreate(&mut admin, &slot_db(slot), Some(TEMPLATE))?;
    }

    Ok(())
}

/// Database name for nextest's `NEXTEST_TEST_GLOBAL_SLOT`.
fn slot_db(slot: usize) -> String {
    format!("mempool_test_{slot}")
}

/// Drops `name` if present and creates it again, cloning `template` when given.
/// FORCE evicts connections left behind by a run that died mid-test.
fn recreate(conn: &mut PgConnection, name: &str, template: Option<&str>) -> Result<(), BoxError> {
    sql_query(format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)")).execute(conn)?;
    let create = match template {
        Some(template) => format!("CREATE DATABASE {name} TEMPLATE {template}"),
        None => format!("CREATE DATABASE {name}"),
    };
    sql_query(create).execute(conn)?;
    Ok(())
}
