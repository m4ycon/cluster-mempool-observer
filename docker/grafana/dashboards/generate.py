#!/usr/bin/env python3

"""Emits the provisioned Grafana dashboards. One per subsystem.

    python3 docker/grafana/dashboards/generate.py   # from the repo root

The JSON is checked in, but edit here: this owns the gridPos arithmetic.
"""
import json
import os

DS = {"type": "prometheus", "uid": "prometheus"}
OUT_DIR = "docker/grafana/dashboards"

# Tables follow the time picker rather than a hardcoded window.
RANGE = "$__range"


class Dashboard:
    """Accumulates panels, tracking the grid cursor so gridPos stays implicit."""

    def __init__(self, uid, title, description, tag):
        self.uid = uid
        self.title = title
        self.description = description
        self.tag = tag
        self.panels = []
        self.next_id = 1
        self.y = 0

    def pid(self):
        self.next_id += 1
        return self.next_id

    def place(self, panel, x, w, h):
        panel["gridPos"] = {"h": h, "w": w, "x": x, "y": self.y}
        self.panels.append(panel)
        # Advance only once the row is full, so side-by-side panels share a y.
        if x + w >= 24:
            self.y += h

    def row(self, title):
        self.place(
            {
                "id": self.pid(),
                "type": "row",
                "title": title,
                "collapsed": False,
                "panels": [],
            },
            x=0,
            w=24,
            h=1,
        )

    def graph(self, title, queries, unit="short", description="", x=0, w=12, h=8):
        self.place(
            {
                "id": self.pid(),
                "type": "timeseries",
                "title": title,
                "description": description,
                "datasource": DS,
                "fieldConfig": {
                    "defaults": {
                        "unit": unit,
                        "custom": {
                            "lineWidth": 1,
                            "fillOpacity": 8,
                            "showPoints": "never",
                            "axisSoftMin": 0,
                        },
                    },
                    "overrides": [],
                },
                "options": {
                    "legend": {
                        "displayMode": "table",
                        "placement": "bottom",
                        "showLegend": True,
                        "calcs": ["mean", "max"],
                    },
                    "tooltip": {"mode": "multi", "sort": "desc"},
                },
                "targets": [
                    {
                        "datasource": DS,
                        "refId": chr(65 + i),
                        "expr": expr,
                        "legendFormat": legend,
                    }
                    for i, (expr, legend) in enumerate(queries)
                ],
            },
            x,
            w,
            h,
        )

    def rate_graph(self, title, queries, unit="opm", description="", x=0, w=12, h=8):
        """Per-minute rate graph.

        Events here arrive a few times a minute, so per-second rates read as
        0.017. `queries` pass plain per-second rates; the x60 lives here so the
        factor and the unit cannot drift apart.
        """
        self.graph(
            title,
            [(f"60 * ({expr})", legend) for expr, legend in queries],
            unit=unit,
            description=description,
            x=x,
            w=w,
            h=h,
        )

    def stat(self, title, queries, unit="short", description="", x=0, w=12, h=8):
        self.place(
            {
                "id": self.pid(),
                "type": "stat",
                "title": title,
                "description": description,
                "datasource": DS,
                "fieldConfig": {"defaults": {"unit": unit}, "overrides": []},
                "options": {
                    "reduceOptions": {
                        "calcs": ["lastNotNull"],
                        "fields": "",
                        "values": False,
                    },
                    "colorMode": "value",
                    "graphMode": "area",
                    "textMode": "auto",
                },
                "targets": [
                    {
                        "datasource": DS,
                        "refId": chr(65 + i),
                        "expr": expr,
                        "legendFormat": legend,
                    }
                    for i, (expr, legend) in enumerate(queries)
                ],
            },
            x,
            w,
            h,
        )

    def table(self, title, sources, slow, description="", h=9):
        """Latency table: a row per operation, a column per statistic.

        `sources` is (metric, display_name, labels), `labels` being a name, a
        tuple, or None. One source keys rows by its own labels; several union
        into an `operation` column. List every label the metric carries or the
        aggregation merges unrelated rows -- see `check_labels`.

        `slow` is the seconds at which these operations are bad, per table
        because 50ms is a slow query and a fine block reconciliation.
        """
        for metric, _, labels in sources:
            check_labels(metric, labels)

        single = len(sources) == 1 and sources[0][2] is not None
        targets = []
        for i, (stat_name, build) in enumerate(STATS):
            if single:
                metric, _, labels = sources[0]
                expr = build(metric, as_list(labels))
            else:
                expr = "\n  or\n".join(
                    as_operation(build(metric, as_list(labels)), name, labels)
                    for metric, name, labels in sources
                )
            if stat_name != "calls":
                # No samples in range means NaN quantiles and a 0/0 mean, which
                # no colour scale can place and so render as healthy. Every NaN
                # comparison is false, `>=` included, so this drops them while
                # keeping a genuine zero.
                expr = f"({expr}) >= 0"
            targets.append(
                {
                    "datasource": DS,
                    "refId": chr(65 + i),
                    "expr": expr,
                    "instant": True,
                    "range": False,
                    "format": "table",
                }
            )

        self.place(
            {
                "id": self.pid(),
                "type": "table",
                "title": title,
                "description": description,
                "datasource": DS,
                "fieldConfig": {
                    "defaults": {
                        "unit": "s",
                        "decimals": 4,
                        "min": 0,
                        # For a stat missing on a row that survives elsewhere.
                        "noValue": "-",
                        # Absolute, not data-derived: a relative scale answers
                        # "which is slowest", never "is this bad".
                        "color": {"mode": "thresholds"},
                        "thresholds": thresholds(slow),
                        "custom": {
                            "align": "auto",
                            "filterable": True,
                            # `basic` fills with the threshold colour;
                            # `gradient` shades within one and blurs the steps.
                            "cellOptions": {"type": "color-background", "mode": "basic"},
                        },
                    },
                    "overrides": [
                        {
                            # Not a duration: shading would read as
                            # "busiest is worst".
                            "matcher": {"id": "byName", "options": "calls"},
                            "properties": [
                                {"id": "unit", "value": "short"},
                                {"id": "decimals", "value": 0},
                                {"id": "color", "value": {"mode": "fixed", "fixedColor": "text"}},
                                {
                                    "id": "custom.cellOptions",
                                    "value": {"type": "auto"},
                                },
                                {
                                    "id": "thresholds",
                                    "value": {
                                        "mode": "absolute",
                                        "steps": [{"color": "text", "value": None}],
                                    },
                                },
                            ],
                        }
                    ],
                },
                "options": {
                    "showHeader": True,
                    # Slowest first: finding it is the reason to open the panel.
                    "sortBy": [{"displayName": "p95", "desc": True}],
                },
                "transformations": [
                    # Before the merge: a differing Time column would split one
                    # operation across rows.
                    {
                        "id": "filterFieldsByName",
                        "options": {"exclude": {"names": ["Time"]}},
                    },
                    {"id": "merge", "options": {}},
                    {
                        "id": "organize",
                        "options": {
                            "renameByName": {
                                f"Value #{chr(65 + i)}": stat_name
                                for i, (stat_name, _) in enumerate(STATS)
                            }
                        },
                    },
                ],
                "targets": targets,
            },
            x=0,
            w=24,
            h=h,
        )

    def write(self):
        doc = {
            "uid": self.uid,
            "title": self.title,
            "description": self.description,
            "tags": ["mempool-observer", self.tag],
            "timezone": "browser",
            "editable": True,
            "schemaVersion": 39,
            "version": 1,
            "refresh": "10s",
            "time": {"from": "now-1h", "to": "now"},
            "graphTooltip": 1,
            # Tag-driven: a new dashboard joins the dropdown by existing.
            "links": [
                {
                    "type": "dashboards",
                    "title": "Dashboards",
                    "tags": ["mempool-observer"],
                    "asDropdown": True,
                    "includeVars": False,
                    "keepTime": True,
                    "icon": "external link",
                }
            ],
            "panels": self.panels,
        }
        path = os.path.join(OUT_DIR, f"{self.uid}.json")
        with open(path, "w") as f:
            json.dump(doc, f, indent=2)
            f.write("\n")
        return path, len(self.panels)


# ------------------------------------------------------------ colour scale


def thresholds(slow):
    """Four steps from one budget: quarter fine, half worth noticing, over red."""
    return {
        "mode": "absolute",
        "steps": [
            {"color": "green", "value": None},
            {"color": "yellow", "value": round(slow * 0.25, 6)},
            {"color": "orange", "value": round(slow * 0.5, 6)},
            {"color": "red", "value": slow},
        ],
    }


# ------------------------------------------------------------ query builders


# Seconds at which an operation is a problem, one tier per class of work.
# Tables group by budget, not subsystem: Grafana thresholds are per column, so
# mixing a pool checkout with a mempool delta leaves the fast one 500x too loose.
FAST = 0.05  # in-memory work and a pool checkout
QUERY = 0.25  # one indexed statement or batched write
REQUEST = 1.0 # an HTTP handler reading a snapshot
PIPELINE = 1.0  # applying one delta or resyncing its clusters
NODE = 2.0  # a round trip to Bitcoin Core
STARTUP = 5.0  # replaying 15 days of deltas, once per process

# Labels each table histogram carries. Grouping by fewer merges unrelated rows:
# `db_query_seconds` by `op` alone folds three repositories' `insert` into one.
METRIC_LABELS = {
    "axum_http_requests_duration_seconds": {"method", "status", "endpoint"},
    "db_query_seconds": {"repo", "op"},
    "db_pool_acquire_seconds": set(),
    "mempool_delta_apply_seconds": set(),
    "mempool_delta_stage_seconds": {"stage"},
    "cluster_sync_seconds": {"stage"},
    "cluster_confirm_mined_seconds": {"stage"},
    "cluster_snapshot_build_seconds": set(),
    "home_stats_seconds": set(),
    "bootstrap_stage_seconds": {"stage"},
    "rpc_call_seconds": {"method"},
    "watcher_poll_seconds": {"subject"},
    "zmq_message_handle_seconds": {"subject"},
}

# Folds that are wanted. HTTP latency is per endpoint whatever the verb and
# status; the status breakdown has its own panel.
FOLDED_ON_PURPOSE = {
    "axum_http_requests_duration_seconds": {"method", "status"},
}


def check_labels(metric, labels):
    """Fails the build when a table would silently merge distinct rows."""
    expected = METRIC_LABELS[metric]
    grouped = set(as_list(labels) or [])
    dropped = expected - grouped - FOLDED_ON_PURPOSE.get(metric, set())
    assert not dropped, (
        f"{metric}: table groups by {sorted(grouped) or 'nothing'} but the metric "
        f"carries {sorted(expected)} -- {sorted(dropped)} would be merged away. "
        f"Add them, or declare the fold in FOLDED_ON_PURPOSE."
    )
    assert not grouped - expected, (
        f"{metric}: table groups by {sorted(grouped - expected)}, which the metric "
        f"does not carry"
    )


def as_list(labels):
    """Normalises a label name, tuple of names, or None into a list."""
    if labels is None:
        return None
    return [labels] if isinstance(labels, str) else list(labels)


def quantile_expr(q, metric, by):
    labels = ", ".join((list(by) if by else []) + ["le"])
    return f"histogram_quantile({q}, sum by ({labels}) (rate({metric}_bucket[{RANGE}])))"


def mean_expr(metric, by):
    grouping = f"sum by ({', '.join(by)})" if by else "sum"
    return (
        f"{grouping} (rate({metric}_sum[{RANGE}])) "
        f"/ {grouping} (rate({metric}_count[{RANGE}]))"
    )


def calls_expr(metric, by):
    grouping = f"sum by ({', '.join(by)})" if by else "sum"
    # `increase` extrapolates, so a once-per-start counter yields a fraction near
    # a range edge -- rounded, that showed 0 beside real latency. Ceiling keeps the
    # count from contradicting the quantiles, at the cost of one on busy rows.
    return f"ceil({grouping} (increase({metric}_count[{RANGE}])))"


def as_operation(expr, name, src_labels=None):
    """Relabels a series so unrelated histograms share one table.

    The outer aggregation drops the original label: each group holds one series,
    so the sum is that series' value.
    """
    src_labels = as_list(src_labels)
    if src_labels:
        # Folding several labels into one name would need a label_replace per
        # label and a agreed separator. No metric in a unioned table needs it,
        # so it is rejected rather than half-implemented.
        assert len(src_labels) == 1, (
            f"{name}: unioned tables support one label, got {src_labels}"
        )
        relabelled = (
            f'label_replace({expr}, "operation", "{name} / $1", "{src_labels[0]}", "(.*)")'
        )
    else:
        relabelled = f'label_replace({expr}, "operation", "{name}", "", "")'
    return f"sum by (operation) ({relabelled})"


# Quantiles are interpolated from the shared bucket ladder, so they are only as
# fine as the buckets around them. `avg` is exact by comparison, being
# sum/count -- and hides the tail the quantiles exist to show.
STATS = [
    ("p75", lambda m, by: quantile_expr(0.75, m, by)),
    ("p95", lambda m, by: quantile_expr(0.95, m, by)),
    ("p99", lambda m, by: quantile_expr(0.99, m, by)),
    ("avg", lambda m, by: mean_expr(m, by)),
    ("calls", lambda m, by: calls_expr(m, by)),
]


def p95_graph(metric, by=None, window="5m"):
    labels = ", ".join((list(by) if by else []) + ["le"])
    return f"histogram_quantile(0.95, sum by ({labels}) (rate({metric}_bucket[{window}])))"


# ------------------------------------------------------------------ overview

overview = Dashboard(
    "mempool-overview",
    "Overview",
    "Every timed operation in one place, slowest first. Start here, then open the dashboard for whichever subsystem stands out.",
    "overview",
)
overview.row("Latency summary (p75 / p95 / p99 / avg)")
overview.table(
    f"Fast paths -- budget {FAST * 1000:.0f}ms",
    [
        ("db_pool_acquire_seconds", "db pool acquire", None),
        ("cluster_snapshot_build_seconds", "cluster snapshot build", None),
        ("zmq_message_handle_seconds", "zmq handle", "subject"),
    ],
    slow=FAST,
    description="Work that touches neither the database nor the node, plus the pool checkout itself. All of it should be sub-millisecond, so this is the table with the least slack -- a pool checkout drifting into tens of milliseconds is contention, long before anything else notices.",
    h=7,
)
overview.table(
    f"Database queries -- budget {QUERY * 1000:.0f}ms",
    [("db_query_seconds", "query", ("repo", "op"))],
    slow=QUERY,
    description="Query time only. The pool wait is in the fast-paths table, because a slow query and a starved pool need different fixes.",
    h=13,
)
overview.table(
    f"HTTP requests -- budget {REQUEST:.0f}s",
    [("axum_http_requests_duration_seconds", "http", "endpoint")],
    slow=REQUEST,
    description="Method and status are folded together on purpose: this is how long the endpoint takes, whatever the outcome. Websocket routes are excluded from the layer, so they never appear here.",
)
overview.table(
    f"Pipeline work -- budget {PIPELINE:.0f}s",
    [
        ("mempool_delta_apply_seconds", "delta apply", None),
        ("mempool_delta_stage_seconds", "delta stage", "stage"),
        ("cluster_sync_seconds", "cluster sync", "stage"),
        ("cluster_confirm_mined_seconds", "confirm mined", "stage"),
        ("home_stats_seconds", "home stats", None),
    ],
    slow=PIPELINE,
    description="One delta applied, or one round of cluster work. Cluster sync stages are skipped when they have no work, so a missing row means an idle round rather than a broken timer.",
    h=11,
)
overview.table(
    f"Node calls -- budget {NODE:.0f}s",
    [
        ("rpc_call_seconds", "rpc", "method"),
        ("watcher_poll_seconds", "watcher poll", "subject"),
    ],
    slow=NODE,
    description="RPC rows include failed calls: the time was spent either way.",
    h=8,
)
overview.table(
    f"Startup -- budget {STARTUP:.0f}s",
    [("bootstrap_stage_seconds", "bootstrap", "stage")],
    slow=STARTUP,
    description="Runs once per process start and replays 15 days of deltas, so seconds here are expected in a way they are nowhere else. Kept out of the pipeline table for exactly that reason -- sharing its budget would have hidden a slow delta stage.",
    h=7,
)

overview.row("Health")
overview.graph(
    "Scrape health",
    [
        ('up{job="mempool-observer"}', "api target up"),
        (
            '60 * sum(rate(scrape_samples_scraped{job="mempool-observer"}[1m]))',
            "samples per minute",
        ),
    ],
    description="0 on the first line means Prometheus cannot reach the admin port -- check METRICS_BIND is not loopback-only. Requires the prometheus self-scrape job.",
    x=0,
    w=24,
)
overview.rate_graph(
    "Error rates",
    [
        ("sum by (repo, op) (rate(db_query_errors_total[5m]))", "db query {{repo}}/{{op}}"),
        (
            "sum by (repo, op) (rate(db_pool_acquire_errors_total[5m]))",
            "db acquire {{repo}}/{{op}}",
        ),
        ("sum by (method) (rate(rpc_call_errors_total[5m]))", "rpc {{method}}"),
        ("sum by (subject) (rate(watcher_poll_errors_total[5m]))", "poll {{subject}}"),
        ("sum by (subject) (rate(pubsub_lagged_total[5m]))", "lagged {{subject}}"),
    ],
    description="Every error counter in the system. Flat at zero is the expected state.",
    x=0,
)
overview.graph(
    "Bootstrap stage duration (mean)",
    [("bootstrap_stage_seconds_sum / bootstrap_stage_seconds_count", "{{stage}}")],
    unit="s",
    description="Runs once per process start, so a mean over the series is the useful view rather than a rate.",
    x=12,
)

# ---------------------------------------------------------------------- HTTP

http = Dashboard(
    "mempool-http",
    "HTTP",
    "Request latency and volume from the axum-prometheus layer. Websocket routes are excluded: their request duration is the client's session length.",
    "http",
)
http.table(
    f"Request latency, by endpoint -- budget {REQUEST:.0f}s",
    [("axum_http_requests_duration_seconds", "http", "endpoint")],
    slow=REQUEST,
    description="Method and status are folded together on purpose: this is how long the endpoint takes, whatever the outcome.",
)
http.row("Traffic")
http.rate_graph(
    "Request rate by endpoint",
    [("sum by (endpoint) (rate(axum_http_requests_total[1m]))", "{{endpoint}}")],
    unit="cpm",
    x=0,
)
http.graph(
    "Latency p95 by endpoint",
    [(p95_graph("axum_http_requests_duration_seconds", ["endpoint"]), "{{endpoint}}")],
    unit="s",
    x=12,
)
http.rate_graph(
    "Non-2xx responses",
    [
        (
            'sum by (status, endpoint) (rate(axum_http_requests_total{status!~"2.."}[5m]))',
            "{{status}} {{endpoint}}",
        )
    ],
    unit="cpm",
    description='Unrouted paths all land on endpoint="unmatched", so a scanner cannot mint a series per URL.',
    x=0,
)
http.graph(
    "Requests in flight",
    [("sum by (endpoint) (axum_http_requests_pending)", "{{endpoint}}")],
    x=12,
)

# ------------------------------------------------------------------ database

database = Dashboard(
    "mempool-database",
    "Database",
    "Postgres access through api/src/db/instrument.rs. Pool wait and query time are measured separately, because a slow repository call means either the database is slow or every connection is busy -- and those need different fixes.",
    "database",
)
database.table(
    f"Query latency, by repository and operation -- budget {QUERY * 1000:.0f}ms",
    [("db_query_seconds", "query", ("repo", "op"))],
    slow=QUERY,
    h=13,
)
database.table(
    f"Pool checkout -- budget {FAST * 1000:.0f}ms",
    [("db_pool_acquire_seconds", "db pool acquire", None)],
    slow=FAST,
    description="Its own table because its budget is a fifth of a query's: waiting tens of milliseconds for a connection is contention, while the same number in a query is unremarkable. Sharing a threshold would have hidden it.",
    h=6,
)
database.row("Pool")
database.graph(
    "Query p95 by repository and operation",
    [(p95_graph("db_query_seconds", ["repo", "op"]), "{{repo}}/{{op}}")],
    unit="s",
    x=0,
)
database.graph(
    "Pool acquire p95",
    [(p95_graph("db_pool_acquire_seconds"), "acquire")],
    unit="s",
    description="Rising here means the pool is the bottleneck, not the query.",
    x=12,
)
database.graph(
    "Pool occupancy",
    [("db_pool_connections", "{{state}}")],
    description="waiting > 0 means callers are queued behind the pool.",
    x=0,
)
database.rate_graph(
    "Errors",
    [
        ("sum by (repo, op) (rate(db_query_errors_total[5m]))", "query {{repo}}/{{op}}"),
        (
            "sum by (repo, op) (rate(db_pool_acquire_errors_total[5m]))",
            "acquire {{repo}}/{{op}}",
        ),
    ],
    description="Acquire errors are pool exhaustion or an unreachable database -- the clearest backpressure signal there is.",
    x=12,
)

# ------------------------------------------------------------------- mempool

mempool = Dashboard(
    "mempool-pipeline",
    "Mempool pipeline",
    "Applying one mempool delta: counting the txids, inserting the ones not already stored as hollow rows, queueing them for backfill, then resyncing their clusters.",
    "mempool",
)
mempool.table(
    f"Delta work -- budget {PIPELINE:.0f}s",
    [
        ("mempool_delta_apply_seconds", "apply (whole delta)", None),
        ("mempool_delta_stage_seconds", "delta stage", "stage"),
        ("home_stats_seconds", "home stats", None),
    ],
    slow=PIPELINE,
    h=8,
)
mempool.row("Throughput")
mempool.graph(
    "Delta apply p95",
    [(p95_graph("mempool_delta_apply_seconds"), "apply")],
    unit="s",
    x=0,
)
mempool.graph(
    "Stage p95",
    [(p95_graph("mempool_delta_stage_seconds", ["stage"]), "{{stage}}")],
    unit="s",
    x=12,
)
mempool.rate_graph(
    "Transaction throughput",
    [
        ("sum by (direction) (rate(mempool_delta_txs_total[1m]))", "{{direction}}"),
        ("rate(mempool_new_txs_total[1m])", "newly fetched"),
    ],
    description="Gap between added and newly fetched is the dedup rate against already-stored txids.",
    x=0,
)
mempool.graph(
    "Home stats p95",
    [(p95_graph("home_stats_seconds"), "current_stats")],
    unit="s",
    x=12,
)
mempool.rate_graph(
    "Backfill throughput",
    [
        ("rate(mempool_new_txs_total[1m])", "enqueued"),
        ("rate(tx_backfill_total[1m])", "enriched"),
        ("rate(tx_backfill_queue_dropped_total[1m])", "dropped, queue full"),
    ],
    description=(
        "Every newly-stored tx is inserted hollow and enqueued, so enqueued tracks "
        "new txs. Enriched trailing it is expected -- the node is pruned, so a tx "
        "that confirms or is evicted before the consumer reaches it can never be "
        "fetched. Dropped should sit at zero; anything else means the queue "
        "capacity is too small for the bootstrap burst."
    ),
    x=0,
    w=24,
)

# ------------------------------------------------------------------ clusters

clusters = Dashboard(
    "mempool-clusters",
    "Clusters",
    "Cluster resync rounds, block reconciliation, and the active-cluster snapshot served to new subscribers.",
    "clusters",
)
clusters.table(
    f"Round stages -- budget {PIPELINE:.0f}s",
    [
        ("cluster_sync_seconds", "sync", "stage"),
        ("cluster_confirm_mined_seconds", "confirm mined", "stage"),
    ],
    slow=PIPELINE,
    h=8,
)
clusters.table(
    f"Snapshot build -- budget {FAST * 1000:.0f}ms",
    [("cluster_snapshot_build_seconds", "snapshot build", None)],
    slow=FAST,
    description="A clone of the active set, in memory, once per new subscriber. Nothing here touches the database, so its budget is far tighter than a resync round's.",
    h=6,
)
clusters.row("Rounds")
clusters.graph(
    "Resync stage p95",
    [(p95_graph("cluster_sync_seconds", ["stage"]), "{{stage}}")],
    unit="s",
    description="Stages with no work are skipped, so a missing line means an idle pipeline rather than a broken timer.",
    x=0,
)
clusters.graph(
    "Confirm-mined stage p95",
    [(p95_graph("cluster_confirm_mined_seconds", ["stage"]), "{{stage}}")],
    unit="s",
    x=12,
)
clusters.rate_graph(
    "Deltas published",
    [("sum by (kind) (rate(cluster_delta_published_total[1m]))", "{{kind}}")],
    description="Only changes that survived the diff against the snapshot.",
    x=0,
)
clusters.stat("Active clusters", [("cluster_active_count", "active")], x=12, w=6)
clusters.graph(
    "Snapshot build p95",
    [(p95_graph("cluster_snapshot_build_seconds"), "per subscriber")],
    unit="s",
    description="Paid once per new websocket subscriber.",
    x=18,
    w=6,
)

# ------------------------------------------------------------------ observer

observer = Dashboard(
    "mempool-observer-ingest",
    "Ingest",
    "Everything that talks to Bitcoin Core: RPC calls, the polling watcher, and the ZMQ block stream.",
    "ingest",
)
observer.table(
    f"Node calls -- budget {NODE:.0f}s",
    [
        ("rpc_call_seconds", "rpc", "method"),
        ("watcher_poll_seconds", "watcher poll", "subject"),
    ],
    slow=NODE,
    description="RPC rows include failed calls: the time was spent either way.",
    h=8,
)
observer.table(
    f"ZMQ handling -- budget {FAST * 1000:.0f}ms",
    [("zmq_message_handle_seconds", "zmq handle", "subject")],
    slow=FAST,
    description="Decoding a block hash and publishing it. No network round trip, so it belongs to the fast tier rather than with the RPC calls it used to share a threshold with.",
    h=6,
)
observer.row("RPC")
observer.graph(
    "RPC p95 by method",
    [(p95_graph("rpc_call_seconds", ["method"]), "{{method}}")],
    unit="s",
    x=0,
)
observer.rate_graph(
    "RPC errors",
    [("sum by (method) (rate(rpc_call_errors_total[5m]))", "{{method}}")],
    x=12,
)
observer.row("Watchers")
observer.graph(
    "Poll p95 by subject",
    [(p95_graph("watcher_poll_seconds", ["subject"]), "{{subject}}")],
    unit="s",
    x=0,
)
observer.rate_graph(
    "Poll failures",
    [
        ("sum by (subject) (rate(watcher_poll_errors_total[5m]))", "errors {{subject}}"),
        (
            "sum by (subject) (rate(watcher_poll_overruns_total[5m]))",
            "overruns {{subject}}",
        ),
    ],
    description="Overruns mean the poll consumed its whole interval, so the watcher is behind its configured rate. Errors are swallowed by the loop -- without this, a dead node looks like an idle mempool.",
    x=12,
)
observer.row("ZMQ")
observer.rate_graph(
    "Message rate",
    [
        ("sum by (subject) (rate(zmq_messages_total[5m]))", "messages {{subject}}"),
        (
            "sum by (subject, kind) (rate(zmq_errors_total[5m]))",
            "errors {{subject}} {{kind}}",
        ),
        ("sum by (subject) (rate(zmq_reconnects_total[5m]))", "reconnects {{subject}}"),
    ],
    description="Reconnects climbing means the node connection is flapping, which no latency metric would show.",
    x=0,
)
observer.graph(
    "Handle p95",
    [(p95_graph("zmq_message_handle_seconds", ["subject"]), "{{subject}}")],
    unit="s",
    x=12,
)

# ----------------------------------------------------------------- event bus

bus = Dashboard(
    "mempool-bus",
    "Event bus",
    "The in-process broadcast bus in shared/src/pubsub.rs, carrying events from the watchers to the api and out to websocket clients.",
    "bus",
)
bus.graph(
    "Queue depth",
    [("pubsub_queue_depth", "{{subject}}")],
    description="Messages the slowest subscriber has not read. Climbing toward 1024 (CHANNEL_CAPACITY) is the backpressure signal that precedes lagging.",
    x=0,
)
bus.rate_graph(
    "Lagged messages",
    [("sum by (subject) (rate(pubsub_lagged_total[5m]))", "{{subject}}")],
    description="Dropped and never replayed. Anything above zero is data a client did not receive.",
    x=12,
)
bus.rate_graph(
    "Publish rate",
    [("sum by (subject) (rate(pubsub_published_total[1m]))", "{{subject}}")],
    x=0,
)
bus.graph(
    "Subscribers",
    [("pubsub_subscribers", "{{subject}}")],
    description="Live receivers per subject -- the fanout each publish pays for.",
    x=12,
)
bus.rate_graph(
    "Events dropped before the bus",
    [
        (
            "sum by (subject) (rate(event_serialize_errors_total[5m]))",
            "serialize errors {{subject}}",
        )
    ],
    description="Serialization failed, so the event never reached the bus and nothing downstream can report it.",
    x=0,
    w=24,
)


for dashboard in (overview, http, database, mempool, clusters, observer, bus):
    path, count = dashboard.write()
    print(f"{path}: {count} panels")
