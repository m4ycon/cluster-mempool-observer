ALTER TABLE blocks RENAME COLUMN total_size TO total_bytes;
ALTER TABLE clusters RENAME COLUMN total_size TO total_vsize;
