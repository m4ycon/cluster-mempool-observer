CREATE TABLE clusters (
    id            BIGSERIAL   PRIMARY KEY,
    txids         TEXT[]      NOT NULL,
    total_fee     BIGINT      NOT NULL,
    first_seen_at TIMESTAMPTZ,
    confirmed_at  TIMESTAMPTZ
);

ALTER TABLE transactions
    ADD COLUMN cluster_id BIGINT REFERENCES clusters(id) ON DELETE SET NULL;

CREATE INDEX idx_transactions_cluster_id ON transactions (cluster_id);
