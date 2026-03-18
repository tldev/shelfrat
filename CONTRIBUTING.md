# Contributing to shelfrat

## Finding issues

Check [GitHub Issues](https://github.com/tldev/shelfrat/issues) for open tasks. Issues labeled `good first issue` are a great starting point.

## Development environment

### Prerequisites

- Rust 1.88+ (`rustup` recommended)
- Node.js 22+ (`nvm` recommended)
- SQLite3

### Setup

```sh
git clone https://github.com/tldev/shelfrat.git
cd shelfrat

# Backend dependencies
cargo build

# Frontend dependencies
cd web && npm install
```

### Running locally

```sh
# Terminal 1: backend
LIBRARY_PATH=/path/to/books cargo run

# Terminal 2: frontend dev server
cd web && npm run dev
```

The frontend dev server runs on `localhost:5173` and proxies `/api` to `localhost:3000`.

## Code organization

```
crates/shelfrat/src/
  api/          HTTP handlers (axum routes)
  services/     Business logic
  repositories/ Database queries (SeaORM + SQLx)
  entities/     SeaORM entity definitions
  config.rs     ENV-var-driven configuration
  auth.rs       JWT claims, AuthUser/AdminUser extractors
  email.rs      SMTP configuration and sending
  jobs.rs       Background job scheduler
  scanner.rs    Library directory scanner
  metaqueue.rs  Background metadata enrichment queue

web/src/
  lib/          Shared components, API client, auth state
  routes/       SvelteKit pages
```

## Commit format

PR titles must follow [conventional commits](https://www.conventionalcommits.org/). CI enforces this.

**Types:** `feat`, `fix`, `chore`, `docs`, `refactor`, `test`, `ci`, `perf`, `build`, `style`

**Optional scopes:** `backend`, `frontend`, `docker`, `ci`, `deps`

Examples:
- `feat(backend): add OPDS catalog endpoint`
- `fix(frontend): handle empty search results`
- `chore(deps): update sveltekit to 2.51`

## PR checklist

- [ ] Tests pass (`cargo test --all` and `cd web && npx vitest run`)
- [ ] Code is formatted (`cargo fmt --all` and `cd web && npm run check`)
- [ ] Clippy is clean (`cargo clippy --all-targets --all-features`)
- [ ] PR title follows conventional commit format
- [ ] New features include tests where practical
- [ ] Breaking changes are noted in the PR description
