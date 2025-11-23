# ==============================================================================
# Dockerfile - Genetics Frontend
# ==============================================================================
# Description: Static nginx frontend for genetics processing job submission
# Author: Matt Barham
# Created: 2025-11-18
# Modified: 2025-11-18
# Version: 1.0.0
# Base Image: nginx:1.27.3-alpine
# Security: Non-root user (genetics:genetics)
# ==============================================================================

FROM nginx:1.27.3-alpine

# Build arguments for user/group
ARG USER_ID=3000
ARG GROUP_ID=3000
ARG USERNAME=genetics
ARG GROUPNAME=genetics

# Add non-root user and group
RUN addgroup -g ${GROUP_ID} -S ${GROUPNAME} \
    && adduser -u ${USER_ID} -D -S -G ${GROUPNAME} ${USERNAME}

# Copy nginx configuration and configure PID file location for non-root user
COPY nginx.conf /etc/nginx/conf.d/default.conf
RUN sed -i 's|pid.*\/run\/nginx\.pid;|pid /tmp/nginx.pid;|g' /etc/nginx/nginx.conf

# Copy static frontend files
COPY www/ /usr/share/nginx/html/

# Set permissions for nginx directories
RUN mkdir -p /var/cache/nginx/client_temp \
    /var/cache/nginx/proxy_temp \
    /var/cache/nginx/fastcgi_temp \
    /var/cache/nginx/uwsgi_temp \
    /var/cache/nginx/scgi_temp \
    && chown -R ${USERNAME}:${GROUPNAME} /var/cache/nginx \
    && chown -R ${USERNAME}:${GROUPNAME} /var/log/nginx \
    && chown -R ${USERNAME}:${GROUPNAME} /etc/nginx/conf.d \
    && mkdir -p /run \
    && touch /run/nginx.pid \
    && chown -R ${USERNAME}:${GROUPNAME} /run/nginx.pid \
    && chmod -R 755 /usr/share/nginx/html

# Switch to non-root user
USER ${USERNAME}

# Expose port 80
EXPOSE 80

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:80/ || exit 1

# Start nginx
CMD ["nginx", "-g", "daemon off;"]
