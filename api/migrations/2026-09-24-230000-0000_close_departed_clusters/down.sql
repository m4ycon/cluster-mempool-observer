-- Irreversible data fix: the pre-fix state is not worth restoring. The
-- statement is there because diesel rejects a down.sql with nothing to run.
SELECT 1;
