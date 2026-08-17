-- Append-only log of process/node lifecycle facts, so mempool_deltas/cluster_deltas
-- history can be told apart from artifacts of the api restarting or the node going away.
CREATE TYPE system_event_kind AS ENUM (
    'server_started',
    'server_stopped',
    'bootstrap_started',
    'bootstrap_completed',
    'node_connected',
    'node_disconnected',
    'node_version_changed'
);

CREATE TABLE system_events (
    id         BIGSERIAL         PRIMARY KEY,
    kind       system_event_kind NOT NULL,
    details    JSONB             NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ       NOT NULL DEFAULT now()
);

CREATE INDEX system_events_created_at_idx ON system_events (created_at);
CREATE INDEX system_events_kind_created_at_idx ON system_events (kind, created_at);
