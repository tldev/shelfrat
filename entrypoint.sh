#!/bin/sh
set -e

# Timezone
if [ -n "$TZ" ] && [ -f "/usr/share/zoneinfo/$TZ" ]; then
    ln -sf "/usr/share/zoneinfo/$TZ" /etc/localtime
    echo "$TZ" > /etc/timezone
fi

PUID=${PUID:-1000}
PGID=${PGID:-1000}

groupmod -o -g "$PGID" shelf 2>/dev/null || true
usermod -o -u "$PUID" shelf 2>/dev/null || true

chown -R shelf:shelf /app /data

exec gosu shelf ./shelfrat
