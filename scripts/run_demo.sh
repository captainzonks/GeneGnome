#!/usr/bin/env bash
# ==============================================================================
# run_demo.sh - Run GeneGnome on synthetic data
# ==============================================================================
# Description: Starts, tests or removes the self-contained demo stack in demo/.
#              Needs only Docker with Compose v2.
# Author: Matt Barham
# Created: 2026-11-26
# Modified: 2026-11-26
# Version: 1.0.0
# Requirements: docker (Compose v2)
# Usage: scripts/run_demo.sh [up|test|down]
#          up    build and start the demo (default)
#          test  start it, then run one synthetic job end to end
#          down  stop it and delete its data
# ==============================================================================

set -euo pipefail
IFS=$'\n\t'

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly demo_dir="${repo_root}/demo"
readonly port="${DEMO_PORT:-8088}"
readonly mail_port="${DEMO_MAIL_PORT:-8025}"

DEMO_UID="$(id -u)"
DEMO_GID="$(id -g)"
export DEMO_UID DEMO_GID

compose() {
    docker compose --project-directory "${demo_dir}" -f "${demo_dir}/docker-compose.yml" "$@"
}

# Throwaway secrets, generated once. World-readable so that the non-root
# containers can read them; they protect nothing but this synthetic demo.
create_secrets() {
    local name
    mkdir -p "${demo_dir}/secrets" "${demo_dir}/data"
    for name in owner_password app_password smtp_password; do
        [[ -s "${demo_dir}/secrets/${name}" ]] && continue
        head -c 24 /dev/urandom | od -An -tx1 | tr -d ' \n' >"${demo_dir}/secrets/${name}"
        chmod 0644 "${demo_dir}/secrets/${name}"
    done
}

start() {
    create_secrets
    compose up --build --detach --wait proxy worker mailpit
}

case "${1:-up}" in
    up)
        start
        printf '\nDemo running.\n'
        printf '  Web interface:  http://localhost:%s/process\n' "${port}"
        printf '  Captured mail:  http://localhost:%s\n' "${mail_port}"
        printf '  Files to upload: %s\n' "${demo_dir}/data"
        printf 'Stop and delete it with: scripts/run_demo.sh down\n'
        ;;
    test)
        start
        if ! compose run --rm smoke; then
            compose logs --no-color --tail 80 api worker >&2
            exit 1
        fi
        ;;
    down)
        compose --profile test down --volumes --remove-orphans
        rm -f -- "${demo_dir}"/data/*.vcf.gz "${demo_dir}"/data/genome_demo.txt \
            "${demo_dir}"/data/scores.txt "${demo_dir}"/data/expected.json \
            "${demo_dir}"/secrets/*_password
        ;;
    *)
        printf 'Usage: %s [up|test|down]\n' "$0" >&2
        exit 2
        ;;
esac
