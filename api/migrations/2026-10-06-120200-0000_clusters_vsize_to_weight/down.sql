ALTER TABLE clusters ALTER COLUMN total_weight TYPE BIGINT USING (total_weight + 3) / 4;
ALTER TABLE clusters RENAME COLUMN total_weight TO total_vsize;
