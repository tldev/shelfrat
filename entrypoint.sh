#!/bin/sh
set -e

PUID=${PUID:-1000}
PGID=${PGID:-1000}

groupmod -o -g "$PGID" shelf 2>/dev/null || true
usermod -o -u "$PUID" shelf 2>/dev/null || true

chown -R shelf:shelf /app /data

exec gosu shelf ./shelfrat
