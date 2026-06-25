CREATE TABLE blocks (
    hash       TEXT             PRIMARY KEY,
    height     BIGINT           NOT NULL,
    mined_at   TIMESTAMPTZ      NOT NULL,
    tx_count   BIGINT           NOT NULL,
    total_size BIGINT           NOT NULL,
    total_fee  BIGINT           NOT NULL,
    difficulty DOUBLE PRECISION NOT NULL
);
