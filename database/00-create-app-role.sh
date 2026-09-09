#!/usr/bin/env bash
# ==============================================================================
# 00-create-app-role.sh - Create the non-superuser runtime database role
# ==============================================================================
# Description: Creates genetics_app (LOGIN, not superuser, not BYPASSRLS,
#              owns nothing) with a password sourced from a Docker secret.
#              Runs automatically on a fresh volume via
#              docker-entrypoint-initdb.d (sorts before init.sql - "0" < "i").
#              For an already-initialized volume, run it manually:
#                docker exec postgres18-genetics bash /docker-entrypoint-initdb.d/00-create-app-role.sh
#              then apply database/migrations/004_separate_runtime_role_from_owner.sql.
# Author: Matt Barham
# Created: 2026-09-08
# Modified: 2026-09-08
# Version: 1.0.0
# ==============================================================================

set -euo pipefail

: "${GENETICS_APP_DB_PASSWORD_FILE:?GENETICS_APP_DB_PASSWORD_FILE must be set}"
: "${POSTGRES_USER:?POSTGRES_USER must be set}"
: "${POSTGRES_DB:?POSTGRES_DB must be set}"

APP_PASSWORD="$(cat "$GENETICS_APP_DB_PASSWORD_FILE")"


# NOTE: psql's :'var' substitution does not happen inside dollar-quoted
# ($$...$$) blocks, so a DO $$ ... $$ block can't reference :'app_password'
# directly - it would be sent to the server as the literal text
# :'app_password' and fail with a syntax error. \gexec (build the command
# as a string with a top-level, correctly-substituted+quoted format() call,
# then execute it) avoids that entirely.
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
    -v app_password="$APP_PASSWORD" <<-'EOSQL'
    SELECT format('CREATE ROLE genetics_app LOGIN PASSWORD %L', :'app_password')
    WHERE NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'genetics_app')
    \gexec

    SELECT format('ALTER ROLE genetics_app PASSWORD %L', :'app_password')
    WHERE EXISTS (SELECT FROM pg_roles WHERE rolname = 'genetics_app')
    \gexec
EOSQL

echo "genetics_app role ready."
