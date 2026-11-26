#!/usr/bin/env bash
# ==============================================================================
# ci_rls_test.sh - Run the database integration tests on a throwaway Postgres
# ==============================================================================
# Description: Starts a disposable Postgres container initialized exactly like a
#              fresh deployment (00-create-app-role.sh, then init.sql), runs the
#              RLS and schema contract tests in api-gateway/tests as the runtime
#              role, and removes the container. Used by CI and runnable locally.
# Author: Matt Barham
# Created: 2026-11-25
# Modified: 2026-11-26
# Version: 1.0.2
# Requirements: docker, cargo, openssl
# Usage: scripts/ci_rls_test.sh
# ==============================================================================

set -euo pipefail
IFS=$'\n\t'

readonly POSTGRES_IMAGE="postgres:18.7"
readonly CONTAINER="genegnome_rls_test_$$"
readonly READY_ATTEMPTS=60

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work_dir="$(mktemp -d)"

cleanup() {
    docker rm -f -v "${CONTAINER}" >/dev/null 2>&1 || true
    rm -rf "${work_dir}"
}
trap cleanup EXIT

# Random per run: nothing secret is stored in the repository.
owner_password="$(openssl rand -hex 24)"
app_password="$(openssl rand -hex 24)"
printf '%s' "${app_password}" >"${work_dir}/app_password"
# Copies, so the container user can read them whatever the checkout's umask is.
cp "${repo_root}/database/00-create-app-role.sh" "${repo_root}/database/init.sql" "${work_dir}/"
chmod 0755 "${work_dir}"
chmod 0644 "${work_dir}"/*

POSTGRES_PASSWORD="${owner_password}" docker run -d --name "${CONTAINER}" \
    -p 127.0.0.1::5432 \
    -e POSTGRES_USER=genetics_owner \
    -e POSTGRES_DB=genetics \
    -e POSTGRES_PASSWORD \
    -e GENETICS_APP_DB_PASSWORD_FILE=/run/ci/app_password \
    -v "${work_dir}/app_password:/run/ci/app_password:ro" \
    -v "${work_dir}/00-create-app-role.sh:/docker-entrypoint-initdb.d/00-create-app-role.sh:ro" \
    -v "${work_dir}/init.sql:/docker-entrypoint-initdb.d/init.sql:ro" \
    "${POSTGRES_IMAGE}" >/dev/null

# The init-time server listens only on the Unix socket, so a TCP check passes
# only after initialization has finished.
attempt=0
until docker exec "${CONTAINER}" pg_isready -h 127.0.0.1 -U genetics_owner -d genetics >/dev/null 2>&1; do
    attempt=$((attempt + 1))
    if ((attempt >= READY_ATTEMPTS)); then
        printf 'Postgres did not become ready\n' >&2
        docker logs "${CONTAINER}" >&2
        exit 1
    fi
    sleep 1
done

port="$(docker port "${CONTAINER}" 5432/tcp)"
port="${port##*:}"

cd "${repo_root}/api-gateway"
TEST_DATABASE_URL="postgresql://genetics_app:${app_password}@127.0.0.1:${port}/genetics" \
    cargo test --locked --test rls_enforcement --test schema_contract -- --test-threads=1 --nocapture 2>&1 |
    tee "${work_dir}/test.log"

# The test returns early when it has no database. That must not count as a pass.
if grep -q 'SKIPPED' "${work_dir}/test.log"; then
    printf 'Database tests were skipped\n' >&2
    exit 1
fi
