-- Runs once when the postgres-data volume is first created
-- (docker-entrypoint-initdb.d). Forgejo gets its own database and user —
-- it never shares the clotho control-plane database.
CREATE USER forgejo WITH PASSWORD 'forgejo-dev'; -- dev-only credentials
CREATE DATABASE forgejo OWNER forgejo;
