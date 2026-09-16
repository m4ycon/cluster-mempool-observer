-- Creates and refreshes the read-only role used for external access.
--
-- Run by the db-provision-ro service in compose.db-external.yml, so the role
-- always exists wherever the port is published. Everything here is idempotent:
-- re-running it is also how DB_RO_PASSWORD gets rotated.
--
-- Reading the password from the environment rather than from a -v argument is
-- deliberate: psql's arguments are visible in `ps` on a shared host.
\set ro_password `printenv DB_RO_PASSWORD`

SELECT format('CREATE ROLE %I LOGIN', :'ro_user')
WHERE NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = :'ro_user')
\gexec

ALTER ROLE :"ro_user" WITH LOGIN
    PASSWORD :'ro_password'
    CONNECTION LIMIT 5;

-- The external client shares the instance with the ingest, so a heavy query or
-- a session left open on someone's laptop must not stall the writers.
ALTER ROLE :"ro_user" SET statement_timeout = '60s';
ALTER ROLE :"ro_user" SET idle_in_transaction_session_timeout = '30s';

GRANT CONNECT ON DATABASE :"db_name" TO :"ro_user";
GRANT USAGE ON SCHEMA public TO :"ro_user";
GRANT SELECT ON ALL TABLES IN SCHEMA public TO :"ro_user";

-- Migrations run at api startup as :"db_user", so without this every table a
-- future migration adds is invisible to the read-only role. It is also what
-- makes this safe to run before the api has migrated anything.
ALTER DEFAULT PRIVILEGES FOR ROLE :"db_user" IN SCHEMA public
    GRANT SELECT ON TABLES TO :"ro_user";
