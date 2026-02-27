# RUC Finder

Rust application that scrapes RUC files from DNIT Paraguay, stores them in PostgreSQL, and exposes a REST API with Actix Web. Also supports offline mode, exporting data to CSV, JSON, Parquet, or NEON.

## Architecture

```
src/
├── main.rs          # Entrypoint: CLI (clap) + Actix Web server
├── config.rs        # Config loading (file + env vars + defaults)
├── db.rs            # PostgreSQL connection pool (sqlx)
├── models.rs        # Entities, DTOs, parsing structs
├── repository.rs    # SQL queries (dynamic search with ILIKE)
├── scraper.rs       # ZIP download, in-memory extraction, parsing, upsert/export
├── exporter.rs      # Export to CSV, JSON, NEON, and Parquet
├── handlers.rs      # REST endpoint handlers
└── errors.rs        # Unified error type (AppError)
```

## Prerequisites

- Rust 1.85+
- PostgreSQL (only required for API/DB mode; offline mode does not need a database)

## Configuration

RUC Finder uses a TOML config file. Default path: `/etc/ruc_finder/ruc_finder.conf`

```toml
[server]
host = "0.0.0.0"
port = 3000

[database]
url = "postgres://user:password@localhost:5432/paraguay?sslmode=require"
pool_size = 10

[sync]
interval_hours = 24
batch_size = 1000
page_url = "https://www.dnit.gov.py/web/portal-institucional/listado-de-ruc-con-sus-equivalencias"
# CIDR networks allowed to call POST /api/v1/sync.
# Only IPs within these networks can trigger a sync.
# Empty array = open to all (NOT recommended in production).
# Example: restrict to ECHOSISTEMA_VPC_NETWORK
allowed_networks = ["10.10.0.0/20"]

[paths]
download_dir = "input/tmp"
output_dir = "./output"

[search]
pagination_limit = 25
pagination_max = 200
fuzzy_limit = 25
fuzzy_max = 200
fuzzy_threshold = 0.3
fuzzy_threshold_min = 0.1
fuzzy_threshold_max = 0.9
```

**Precedence order:** CLI args > environment variables > config file > defaults

Environment variables are still supported for compatibility and Docker usage:

| Variable              | Config equivalent       | Default     |
|-----------------------|-------------------------|-------------|
| `DATABASE_URL`        | `database.url`          | —           |
| `DB_POOL_SIZE`        | `database.pool_size`    | `10`        |
| `HOST`                | `server.host`           | `0.0.0.0`   |
| `PORT`                | `server.port`           | `3000`      |
| `SYNC_INTERVAL_HOURS` | `sync.interval_hours`   | `24`        |
| `SYNC_BATCH_SIZE`     | `sync.batch_size`       | `1000`      |
| `SYNC_PAGE_URL`       | `sync.page_url`         | DNIT URL    |
| `DOWNLOAD_DIR`        | `paths.download_dir`    | `input/tmp` |
| `OUTPUT_DIR`          | `paths.output_dir`      | `./output`  |
| `PAGINATION_LIMIT`    | `search.pagination_limit` | `25`      |
| `PAGINATION_MAX`      | `search.pagination_max`   | `200`     |
| `FUZZY_LIMIT`         | `search.fuzzy_limit`    | `25`        |
| `FUZZY_MAX`           | `search.fuzzy_max`      | `200`       |
| `FUZZY_THRESHOLD`     | `search.fuzzy_threshold` | `0.3`      |
| `SYNC_ALLOWED_NETWORKS` | `sync.allowed_networks` | — (open) |
| `RUST_LOG`            | —                       | —           |

## Build

```bash
cargo build --release
```

---

## Usage

### 1. API server

Start the Actix Web API server. Requires `database.url` to be configured.

```bash
# Using config file
./ruc_finder --config ./ruc_finder.conf

# Override host/port via CLI
./ruc_finder --config ./ruc_finder.conf --host 127.0.0.1 --port 8080
```

### 2. Export to file (offline mode)

Scrape DNIT data and export directly to a file, **no database required**.

```bash
./ruc_finder --sync --format csv
./ruc_finder --sync --format json
./ruc_finder --sync --format parquet
./ruc_finder --sync --format neon
./ruc_finder --sync --format csv --output /tmp/ruc_data
```

| Format    | Description                                                                 |
|-----------|-----------------------------------------------------------------------------|
| `csv`     | Standard CSV with headers. Compatible with Excel, pandas, etc.              |
| `json`    | Pretty-printed JSON array.                                                  |
| `parquet` | Apache Parquet columnar format. Ideal for Spark, DuckDB, pandas.            |
| `neon`    | [NEON](https://github.com/EwertonDaniel/neon-neural-efficient-object-notation) strict mode. Optimized for LLMs. |

### 3. Sync to database

Scrape data and upsert into PostgreSQL, then exit.

```bash
./ruc_finder --config ./ruc_finder.conf --sync
```

### 4. Backfill file hashes

Download files from DB metadata and compute missing hashes.

```bash
./ruc_finder --config ./ruc_finder.conf --backfill-hashes
```

### CLI parameters

| Parameter           | Description                                           |
|---------------------|-------------------------------------------------------|
| `-c`, `--config`    | Path to config file (default: `/etc/ruc_finder/ruc_finder.conf`) |
| `--sync`            | Run the scraper and exit (no API server)              |
| `--format`          | Export format: `csv`, `json`, `neon`, `parquet`       |
| `--output`          | Output directory for file exports (default: `./output`) |
| `--host`            | Host/IP to bind the server                            |
| `--port`            | Port to bind the server                               |
| `--backfill-hashes` | Download files and backfill `file_hash`               |

### Freshness check

The scraper always checks for new data before importing:

1. Extracts the reference date from the DNIT page ("Actualizado al ...")
2. **DB mode**: compares with the latest `reference_date` in `ruc_file_metadata`
3. **File mode**: checks if a file with that date already exists
4. Skips import if data is already up to date

---

## Docker

### Build

```bash
docker build -t echosistema/ruc-finder:latest .
```

### Run

```bash
# API server
docker run -d --name ruc-finder \
  -e DATABASE_URL="postgres://..." \
  -e RUST_LOG=info \
  -p 3000:3000 \
  --restart unless-stopped \
  echosistema/ruc-finder:latest

# Sync to database (one-shot)
docker run --rm \
  -e DATABASE_URL="postgres://..." \
  -e RUST_LOG=info \
  echosistema/ruc-finder:latest --sync

# Backfill hashes
docker run --rm \
  -e DATABASE_URL="postgres://..." \
  -e RUST_LOG=info \
  echosistema/ruc-finder:latest --backfill-hashes

# With custom config file
docker run -d --name ruc-finder \
  -v /path/to/ruc_finder.conf:/etc/ruc_finder/ruc_finder.conf:ro \
  -p 3000:3000 \
  --restart unless-stopped \
  echosistema/ruc-finder:latest
```

### CI/CD

Docker images are automatically built and pushed to [DockerHub](https://hub.docker.com/r/echosistema/ruc-finder) on every git tag push (`v*`). Deploy notifications are sent to Discord.

```bash
# Tag and push to trigger a release
git tag v0.1.0
git push origin v0.1.0
```

---

## Deploying as a Linux service (systemd)

### 1. Install

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin ruc_finder
sudo cp target/release/ruc_finder /usr/local/bin/
sudo mkdir -p /etc/ruc_finder
sudo cp ruc_finder.conf /etc/ruc_finder/ruc_finder.conf
sudo chmod 640 /etc/ruc_finder/ruc_finder.conf
# Edit the config with your database credentials
sudo vim /etc/ruc_finder/ruc_finder.conf
```

### 2. Service

```bash
sudo cp ruc_finder.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now ruc_finder
```

### 3. Periodic sync (systemd timer)

```bash
sudo tee /etc/systemd/system/ruc_finder_sync.service << 'EOF'
[Unit]
Description=RUC Finder Sync
After=network.target

[Service]
Type=oneshot
User=ruc_finder
Group=ruc_finder
ExecStart=/usr/local/bin/ruc_finder --sync
EOF
```

```bash
sudo tee /etc/systemd/system/ruc_finder_sync.timer << 'EOF'
[Unit]
Description=Run RUC Finder sync periodically

[Timer]
OnCalendar=daily
Persistent=true
RandomizedDelaySec=1h

[Install]
WantedBy=timers.target
EOF
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now ruc_finder_sync.timer
```

### 4. Verify

```bash
sudo systemctl status ruc_finder
sudo journalctl -u ruc_finder -f
sudo systemctl list-timers ruc_finder_sync.timer
```

---

## API Reference

Interactive API documentation is available via **Swagger UI** at:

```
http://localhost:3000/swagger-ui/
```

The OpenAPI JSON spec is served at `/api-docs/openapi.json`.

### `GET /api/v1/health`

Health check. Returns `200 OK` if the server and database connection are healthy.

```bash
curl http://localhost:3000/api/v1/health
# {"status":"ok"}
```

### `GET /api/v1/ruc/{number}`

Look up a RUC by its exact number.

```bash
curl http://localhost:3000/api/v1/ruc/1000000
```

### `GET /api/v1/ruc`

Search with combinable filters. Text fields use `unaccent() + ILIKE` (accent-insensitive, case-insensitive, partial match). The `status` filter uses exact match for partition pruning.

| Parameter     | Description                              | Match type             |
|---------------|------------------------------------------|------------------------|
| `ruc`         | RUC number (partial)                     | ILIKE                  |
| `name`        | Search in full_name                      | unaccent + ILIKE       |
| `first_names` | First names                              | unaccent + ILIKE       |
| `last_names`  | Last names                               | unaccent + ILIKE       |
| `full_name`   | Full name                                | unaccent + ILIKE       |
| `old_ruc`     | Old RUC number                           | ILIKE                  |
| `status`      | Status (ACTIVO, CANCELADO, etc.)         | Exact (partition pruning) |
| `page`        | Page number (default: 1)                 | —                      |
| `limit`       | Items per page (default: 25, max: 200)   | —                      |

```bash
curl "http://localhost:3000/api/v1/ruc?name=CAÑETE&page=1&limit=10"
curl "http://localhost:3000/api/v1/ruc?name=GONZALEZ&status=ACTIVO"
curl "http://localhost:3000/api/v1/ruc?ruc=100&last_names=CAÑETE"
```

Response:

```json
{
  "data": [
    {
      "id": 1,
      "ruc": "1000000",
      "first_names": "JUANA DEL CARMEN",
      "last_names": "CAÑETE GONZALEZ",
      "full_name": "JUANA DEL CARMEN CAÑETE GONZALEZ",
      "check_digit": "3",
      "old_ruc": "CAGJ761720E",
      "status": "ACTIVO",
      "created_at": "2026-02-01T00:00:00Z",
      "updated_at": "2026-02-01T00:00:00Z",
      "file_metadata_id": 1
    }
  ],
  "page": 1,
  "limit": 10,
  "total": 42
}
```

### `GET /api/v1/ruc/search`

Fuzzy search using `pg_trgm` + `unaccent`. Results ranked by similarity.

| Parameter   | Description                                | Required |
|-------------|--------------------------------------------|----------|
| `query`     | Text for similarity search                 | Yes      |
| `status`    | Filter by status (partition pruning)       | No       |
| `threshold` | Minimum similarity 0.1–0.9 (default: 0.3) | No       |
| `page`      | Page number (default: 1)                   | No       |
| `limit`     | Results per page (default: 25, max: 200)   | No       |

```bash
curl "http://localhost:3000/api/v1/ruc/search?query=JUAN CARLOS LOPES&status=ACTIVO"
```

### `POST /api/v1/sync`

Triggers the scraper in the background. **Restricted by network** — only requests from IPs within `sync.allowed_networks` are accepted (returns `403 Forbidden` otherwise). Configure via the config file or `SYNC_ALLOWED_NETWORKS` env var (comma-separated CIDRs).

```bash
curl -X POST http://localhost:3000/api/v1/sync
```

---

## Security

### Sync endpoint network restriction

The `POST /api/v1/sync` endpoint triggers a full scrape and database upsert. To prevent unauthorized access, it can be restricted to specific CIDR networks via `sync.allowed_networks`.

**Config file** (TOML array):

```toml
[sync]
allowed_networks = ["10.10.0.0/20", "172.16.0.0/12"]
```

**Environment variable** (comma-separated):

```bash
SYNC_ALLOWED_NETWORKS=10.10.0.0/20,172.16.0.0/12
```

**Behavior:**

| Configuration                  | Result                                          |
|--------------------------------|-------------------------------------------------|
| `allowed_networks = []`        | Open to all IPs (no restriction)                |
| Omitted / not set              | Open to all IPs (no restriction)                |
| `allowed_networks = ["10.10.0.0/20"]` | Only IPs in `10.10.0.0/20` can trigger sync |

Requests from IPs outside the allowed networks receive `403 Forbidden`:

```json
{"error": "Sync endpoint is restricted to the internal network"}
```

---

## Database schema

### Table `ruc_file_metadata`

| Column          | Type          | Description               |
|-----------------|---------------|---------------------------|
| id              | SERIAL PK     | Auto-increment            |
| file_name       | VARCHAR(255)  | File name (ruc0.zip)      |
| file_url        | VARCHAR(1024) | Download URL              |
| reference_date  | DATE          | Reference date            |
| file_hash       | BIGINT        | Content hash for change detection |
| last_updated_at | TIMESTAMPTZ   | default now()             |
| created_at      | TIMESTAMPTZ   | default now()             |

### Table `ruc`

| Column           | Type          | Description                      |
|------------------|---------------|----------------------------------|
| id               | SERIAL PK     | Auto-increment                   |
| ruc              | VARCHAR(20)   | RUC number                       |
| first_names      | VARCHAR(255)  | First names                      |
| last_names       | VARCHAR(255)  | Last names                       |
| full_name        | VARCHAR(512)  | Full name                        |
| check_digit      | VARCHAR(5)    | Check digit                      |
| old_ruc          | VARCHAR(20)   | Old RUC number                   |
| status           | VARCHAR(50)   | ACTIVO, CANCELADO, etc.          |
| created_at       | TIMESTAMPTZ   | default now()                    |
| updated_at       | TIMESTAMPTZ   | default now()                    |
| file_metadata_id | INTEGER FK    | References ruc_file_metadata     |

## Data source

[DNIT Paraguay — Listado de RUC con sus equivalencias](https://www.dnit.gov.py/web/portal-institucional/listado-de-ruc-con-sus-equivalencias)

10 ZIP files (`ruc0.zip` through `ruc9.zip`), each containing a TXT with the format:

```
RUC|LAST_NAMES, FIRST_NAMES|CHECK_DIGIT|OLD_RUC|STATUS|
```
