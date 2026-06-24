UPDATE transactions
SET first_seen_at = now()
WHERE first_seen_at IS NULL;

ALTER TABLE transactions
    ALTER COLUMN first_seen_at SET NOT NULL;
