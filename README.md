<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="web/static/favicon-light.svg">
    <img src="web/static/favicon.svg" alt="shelfrat" width="128" height="128">
  </picture>
</p>

# shelfrat

Self-hosted ebook library management. Scan your library, enrich metadata from multiple providers, and send books to Kindle -- all from a clean web interface.

## Demo

https://github.com/user-attachments/assets/4bee9164-8fc8-4490-806c-c98215a66fa8

## Features

- Automatic library scanning with file deduplication (SHA-256)
- Metadata enrichment from OpenLibrary, Google Books, and Hardcover
- Fuzzy matching and ranking across providers
- Send-to-Kindle via SMTP
- Full-text search across titles, authors, and tags
- OIDC single sign-on with role mapping
- Multi-user with invite system and role-based access
- Background job scheduling with configurable cadences
- Supports EPUB, PDF, MOBI, AZW3, and CBZ formats

## Quick start

Create a `docker-compose.yml`:

```yaml
services:
  shelfrat:
    image: ghcr.io/tldev/shelfrat:latest
    container_name: shelfrat
    ports:
      - "3000:3000"
    volumes:
      - data:/data
      - /path/to/your/books:/library:ro
    environment:
      - PUID=1000
      - PGID=1000
      - TZ=UTC
      - DATABASE_URL=sqlite:/data/shelfrat.db
      - LIBRARY_PATH=/library
    restart: unless-stopped

volumes:
  data:
```

```sh
docker compose up -d
```

Open `http://localhost:3000` and complete the setup wizard.

## Configuration

Settings can be configured via environment variables or the admin UI. When a setting is set via environment variable, it takes priority and appears as read-only in the admin UI.

### Startup-only variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `sqlite:shelfrat.db` | SQLite database path |
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `3000` | Listen port |
| `RUST_LOG` | `info` | Log level |
| `PUID` | `1000` | Container user ID |
| `PGID` | `1000` | Container group ID |
| `TZ` | `UTC` | Timezone |

### Runtime settings

These can also be edited in the admin UI when not set via environment variable.

| Variable | Config key | Default | Description |
|---|---|---|---|
| `LIBRARY_PATH` | `library_path` | -- | Ebook library directory |
| `SHELFRAT_SMTP_HOST` | `smtp_host` | -- | SMTP server |
| `SHELFRAT_SMTP_PORT` | `smtp_port` | `587` | SMTP port |
| `SHELFRAT_SMTP_USER` | `smtp_user` | -- | SMTP username |
| `SHELFRAT_SMTP_PASSWORD` | `smtp_password` | -- | SMTP password |
| `SHELFRAT_SMTP_FROM` | `smtp_from` | -- | Sender address |
| `SHELFRAT_SMTP_ENCRYPTION` | `smtp_encryption` | `starttls` | `tls` / `starttls` / `none` |
| `SHELFRAT_KINDLE_FROM_EMAIL` | `kindle_from_email` | -- | Kindle sender address |
| `SHELFRAT_APP_URL` | `app_url` | -- | Public URL of instance |
| `SHELFRAT_OIDC_ISSUER_URL` | `oidc_issuer_url` | -- | OIDC discovery URL |
| `SHELFRAT_OIDC_CLIENT_ID` | `oidc_client_id` | -- | OIDC client ID |
| `SHELFRAT_OIDC_CLIENT_SECRET` | `oidc_client_secret` | -- | OIDC client secret |
| `SHELFRAT_OIDC_AUTO_REGISTER` | `oidc_auto_register` | `true` | Auto-create on OIDC login |
| `SHELFRAT_OIDC_ADMIN_CLAIM` | `oidc_admin_claim` | `groups` | Claim for admin detection |
| `SHELFRAT_OIDC_ADMIN_VALUE` | `oidc_admin_value` | -- | Value granting admin role |
| `SHELFRAT_OIDC_PROVIDER_NAME` | `oidc_provider_name` | -- | Login button label |
| `SHELFRAT_HARDCOVER_API_KEY` | `hardcover_api_key` | -- | Hardcover API key |
| `SHELFRAT_METADATA_PROVIDERS` | `metadata_providers` | -- | Provider order (JSON array) |
| `SHELFRAT_METADATA_RETRY_HOURS` | `metadata_retry_hours` | `24` | Hours between retries |
| `SHELFRAT_JOB_CADENCE_LIBRARY_SCAN` | `job_cadence:library_scan` | `300` | Scan interval (seconds, 0=off) |

## Docker compose examples

### With OIDC (Authentik, Keycloak, etc.)

```yaml
services:
  shelfrat:
    image: ghcr.io/tldev/shelfrat:latest
    ports:
      - "3000:3000"
    volumes:
      - data:/data
      - /path/to/books:/library:ro
    environment:
      - LIBRARY_PATH=/library
      - DATABASE_URL=sqlite:/data/shelfrat.db
      - SHELFRAT_APP_URL=https://shelf.example.com
      - SHELFRAT_OIDC_ISSUER_URL=https://auth.example.com/realms/main
      - SHELFRAT_OIDC_CLIENT_ID=shelfrat
      - SHELFRAT_OIDC_CLIENT_SECRET=your-secret
      - SHELFRAT_OIDC_PROVIDER_NAME=Authentik
    restart: unless-stopped

volumes:
  data:
```

### With SMTP (Send-to-Kindle)

```yaml
services:
  shelfrat:
    image: ghcr.io/tldev/shelfrat:latest
    ports:
      - "3000:3000"
    volumes:
      - data:/data
      - /path/to/books:/library:ro
    environment:
      - LIBRARY_PATH=/library
      - DATABASE_URL=sqlite:/data/shelfrat.db
      - SHELFRAT_SMTP_HOST=smtp.gmail.com
      - SHELFRAT_SMTP_PORT=587
      - SHELFRAT_SMTP_USER=you@gmail.com
      - SHELFRAT_SMTP_PASSWORD=your-app-password
      - SHELFRAT_SMTP_FROM=you@gmail.com
      - SHELFRAT_SMTP_ENCRYPTION=starttls
    restart: unless-stopped

volumes:
  data:
```

## Supported formats

| Format | Extension | Metadata extraction |
|---|---|---|
| EPUB | `.epub` | Title, author, description, cover |
| PDF | `.pdf` | Filename-based |
| MOBI | `.mobi` | Filename-based |
| AZW3 | `.azw3` | Filename-based |
| CBZ | `.cbz` | Filename-based |

All formats are enriched from external metadata providers using title + author matching.

## Development

### Prerequisites

- Docker (for database and full-stack testing)
- Node.js 22+
- Rust 1.88+

### Dev setup

```sh
# Backend
cargo build

# Frontend
cd web && npm install && npm run dev

# Run the backend (from project root)
LIBRARY_PATH=/path/to/books cargo run
```

The frontend dev server proxies API requests to `localhost:3000`.

### Tests

```sh
# Backend
cargo test --all

# Frontend
cd web && npx vitest run
```

### Code style

```sh
cargo fmt --all
cargo clippy --all-targets --all-features
cd web && npm run check
```

### Commits

This project uses [conventional commits](https://www.conventionalcommits.org/) on PR titles. The PR title becomes the commit message on main (squash merge).

Allowed types: `feat`, `fix`, `chore`, `docs`, `refactor`, `test`, `ci`, `perf`, `build`, `style`

Examples:
- `feat: add OPDS feed support`
- `fix: prevent duplicate metadata enrichment`
- `chore(deps): update axum to 0.9`

### Pull requests

1. Fork the repo and create a branch from `main`
2. Make your changes
3. Ensure tests pass and code is formatted
4. Open a PR with a conventional commit title

## License

[MIT](LICENSE)
