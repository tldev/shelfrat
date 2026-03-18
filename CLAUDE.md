# CLAUDE.md

## Project overview

shelfrat is a self-hosted ebook library manager. Rust backend (axum + SQLite) with a SvelteKit frontend (static adapter, client-side only).

## Build & test commands

```sh
# Backend
cargo build                          # compile
cargo test --all                     # run all tests
cargo fmt --all -- --check           # check formatting
cargo clippy --all-targets --all-features  # lint

# Frontend
cd web && npm install                # install deps
cd web && npm run dev                # dev server (port 5173, proxies to 3000)
cd web && npm run build              # production build
cd web && npm run check              # svelte-check (type checking)
cd web && npx vitest run             # run tests

# Full stack
LIBRARY_PATH=/path/to/books cargo run  # backend on :3000
docker compose up --build              # full Docker build
```

## Architecture

- **Workspace**: single crate at `crates/shelfrat` (lib + bin)
- **API layer**: `src/api/` -- axum Router handlers grouped by domain
- **Services**: `src/services/` -- business logic, no direct HTTP concerns
- **Repositories**: `src/repositories/` -- SeaORM entities + SQLx queries
- **Config**: `src/config.rs` -- ENV-var-first config resolution (`SHELFRAT_*` vars override DB)
- **Auth**: JWT-based, `AuthUser` / `AdminUser` axum extractors
- **Frontend**: SvelteKit with Svelte 5 runes (`$state`, `$derived`), static adapter (SSR disabled)

## Key patterns

- **Config priority**: env var > database (`app_config` table) > startup default
- **Error handling**: `AppError` enum with `From` impls, returns JSON `{ "error": "..." }`
- **Settings API**: `GET /api/v1/admin/settings` returns `{ settings, env_locked }` -- frontend disables locked fields
- **Audit logging**: all admin actions written to `audit_log` table via `audit_repo`
- **Squash merges**: PR title becomes the commit message on main
- **Conventional commits**: enforced on PR titles by CI (`amannn/action-semantic-pull-request`)

## Conventions

- No emojis in code or docs
- Monospace UI aesthetic (JetBrains Mono)
- Prefer editing existing files over creating new ones
- Keep the frontend in Svelte 5 idioms (runes, not stores)
- Tests use real SQLite (in-memory), not mocks
- Config keys use snake_case, env vars use SHELFRAT_ prefix with UPPER_SNAKE_CASE
