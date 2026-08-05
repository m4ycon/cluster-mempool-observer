ALTER TABLE transactions ADD COLUMN hollow BOOLEAN NOT NULL DEFAULT FALSE;

-- Rows written before this column existed. vsize = 0 was the only tell, and no
-- real transaction has it.
UPDATE transactions SET hollow = TRUE WHERE vsize = 0;
