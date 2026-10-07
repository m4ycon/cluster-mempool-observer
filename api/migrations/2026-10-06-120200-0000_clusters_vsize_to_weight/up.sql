ALTER TABLE clusters RENAME COLUMN total_vsize TO total_weight;
ALTER TABLE clusters ALTER COLUMN total_weight TYPE BIGINT USING total_weight * 4;
