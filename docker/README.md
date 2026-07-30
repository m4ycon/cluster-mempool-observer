# Dev metrics stack

Prometheus + Grafana for the backend metrics. Dev only -- no authentication, both
ports bound to `127.0.0.1`. Runs neither the app nor the database; it only
scrapes an api already running on the host.

## Run

Prometheus scrapes from inside a container, so the admin listener cannot stay on
loopback. In `.env`:

```
METRICS_BIND=0.0.0.0:3334
```

Then:

```sh
docker compose -f docker/compose.metrics.yml up -d
```

| What | Where |
| --- | --- |
| Grafana (provisioned, no login) | http://localhost:3031 |
| Prometheus UI | http://localhost:9090 |
| Target health | http://localhost:9090/targets, or the Scrape health panel |

`down` to stop, `down -v` to drop the stored series too. Neither container
restarts on its own. Prometheus is set to keep 30 days in a named volume across `down`/`up`.

## Dashboards

They are provisioned read-only into the **Cluster Mempool Observer** folder. UI
edits are discarded on restart -- edit `grafana/dashboards/generate.py` and rerun
it from the repo root. Each has a `Dashboards` dropdown that keeps the current
time range.

| Dashboard | Covers | Recorded in |
| --- | --- | --- |
| Overview | every timed operation, plus all error counters | -- |
| HTTP | request latency and volume | `axum-prometheus` layer |
| Database | query time, pool wait, occupancy | `api/src/db/instrument.rs` |
| Mempool pipeline | delta apply and stages, tx throughput | `api/src/services/mempool.rs` |
| Clusters | resync, reconciliation, snapshot build | `api/src/services/cluster*.rs` |
| Ingest | RPC, watcher polling, ZMQ | `observer/src/` |
| Event bus | queue depth, lag, fanout | `shared/src/pubsub.rs` |

Start on Overview: six tables, tightest budget first. Whichever row is red, open
that subsystem.
