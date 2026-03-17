# Stage 1: Build SvelteKit frontend
FROM node:22-slim AS frontend

WORKDIR /app/web

COPY web/package.json web/package-lock.json ./
RUN npm ci

COPY web/ .
RUN npm run build

# Stage 2: Build Rust backend
FROM rust:1.88-slim AS backend

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y pkg-config libsqlite3-dev libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY migrations/ migrations/

RUN cargo build --release

# Stage 3: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y libsqlite3-0 ca-certificates curl gosu && rm -rf /var/lib/apt/lists/*

RUN useradd -u 1000 -U -s /bin/false -m shelf

WORKDIR /app

COPY --from=backend /app/target/release/shelfrat .
COPY --from=frontend /app/web/build web/build/
COPY entrypoint.sh .

RUN mkdir -p /data && chown -R shelf:shelf /app /data

ENV DATABASE_URL=sqlite:/data/shelfrat.db
ENV HOST=0.0.0.0
ENV PORT=3000
ENV WEB_DIR=web/build
ENV RUST_LOG=info

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/api/v1/health || exit 1

ENTRYPOINT ["./entrypoint.sh"]
