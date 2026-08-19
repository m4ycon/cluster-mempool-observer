#!/usr/bin/env bash
# nextest setup script: makes sure a Postgres is up and holds one freshly
# migrated database per test slot, then publishes its address to the tests.
#
# Set TEST_PG_URL_BASE beforehand to point at a server that is already running
# (CI does this); otherwise the compose `db-test` service is brought up.
set -euo pipefail

cd "${NEXTEST_WORKSPACE_ROOT:-$(dirname "$0")/..}"

if [[ -z "${TEST_PG_URL_BASE:-}" ]]; then
  if ! docker compose version >/dev/null 2>&1; then
    echo "error: docker compose is unavailable, so the test database cannot be started." >&2
    echo "       Start Docker, or point TEST_PG_URL_BASE at a Postgres you manage." >&2
    exit 1
  fi

  docker compose up --detach --wait db-test
  TEST_PG_URL_BASE="postgres://postgres:postgres@127.0.0.1:${TEST_DB_PORT:-5434}"
fi

cargo run --quiet --package testkit --bin test-pg -- \
  "$TEST_PG_URL_BASE" "${NEXTEST_TEST_THREADS:-4}"

echo "TEST_PG_URL_BASE=$TEST_PG_URL_BASE" >> "$NEXTEST_ENV"
