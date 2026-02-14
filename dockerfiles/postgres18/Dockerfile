# ==============================================================================
# DOCKERFILE - PostgreSQL 18 Core Database with Custom User Mapping
# ==============================================================================
# Description: PostgreSQL 18 with custom UID/GID for host permissions
# Author: Matt Barham
# Created: 2025-11-20
# Modified: 2026-02-04
# Version: 18.1-custom
# Host: Rome (Arch Linux / AMD Ryzen 5600x / Intel Arc A380)
# ==============================================================================
# Base Image: postgres:18.1
# Target Architecture: amd64
# Security: custom user mapping (UID:1000, GID:968)
# ==============================================================================
# Build Command:
#   docker build --build-arg USER_ID=1000 --build-arg GROUP_ID=968 \
#     --build-arg USERNAME=postgres --build-arg GROUPNAME=postgres \
#     --build-arg OG_UID=999 --build-arg OG_GID=999 \
#     -t postgres:18.1-custom .
# Run Command:
#   See stacks/database/docker-compose.yml for runtime configuration
# ==============================================================================
# Enhancements:
#   - Custom user/group mapping for host filesystem permissions
#   - Ownership transfer from original postgres UID/GID to custom
# ==============================================================================

FROM postgres:18.1

ARG USER_ID
ARG GROUP_ID

ARG USERNAME
ARG GROUPNAME

ARG OG_UID
ARG OG_GID

SHELL ["/bin/sh", "-c"]

USER root

RUN if [ ${USER_ID:-0} -ne 0 ] && [ ${GROUP_ID:-0} -ne 0 ]; then\
    userdel -f ${USERNAME} &&\
    if getent group ${GROUPNAME}; then groupdel ${GROUPNAME}; fi &&\
    groupadd -g ${GROUP_ID} ${GROUPNAME} &&\
    useradd -l -u ${USER_ID} -g ${GROUPNAME} ${USERNAME} &&\
    install -d -m 0755 -o ${USERNAME} -g ${GROUPNAME} /home/${USERNAME}/ &&\
    chown --changes --silent --no-dereference --recursive \
        --from=${OG_UID}:${OG_GID} ${USER_ID}:${GROUP_ID} \
            /etc/postgresql/ \
            /var/lib/postgresql/ \
            /var/run/postgresql/ \
;fi

USER ${USERNAME}
