# PostgreSQL Database Setup

Complete guide for setting up the PostgreSQL database required by RUC Finder.

## Prerequisites

- PostgreSQL 14+ (partitioning and GIN index support)
- Superuser or database owner access (to create extensions)

## 1. Required Extensions

RUC Finder depends on two PostgreSQL extensions:

| Extension   | Purpose                                                        |
|-------------|----------------------------------------------------------------|
| `pg_trgm`   | Trigram similarity for fuzzy name search (`%` operator, `similarity()`) |
| `unaccent`  | Accent-insensitive text matching (e.g. "CAÑETE" matches "CANETE") |

```sql
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS unaccent;
```

> **Note:** On managed databases (e.g. DigitalOcean, RDS), these extensions are typically available but may need to be enabled by the database owner.

## 2. Immutable Unaccent Function

PostgreSQL's built-in `unaccent()` is marked as `STABLE`, which prevents its use in expression indexes (GIN indexes require `IMMUTABLE` functions). We create an `IMMUTABLE` wrapper:

```sql
CREATE OR REPLACE FUNCTION immutable_unaccent(text)
RETURNS text AS $$
    SELECT public.unaccent('public.unaccent', $1)
$$ LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT;
```

This function is used by:
- **Filtered search** (`GET /api/v1/ruc`): `immutable_unaccent(column) ILIKE immutable_unaccent($1)` for accent-insensitive matching
- **Fuzzy search** (`GET /api/v1/ruc/search`): `similarity(immutable_unaccent(full_name), immutable_unaccent($1))` for trigram matching

## 3. Tables

### `ruc_file_metadata`

Tracks downloaded source files and their reference dates.

```sql
CREATE TABLE IF NOT EXISTS ruc_file_metadata (
    id              SERIAL PRIMARY KEY,
    file_name       VARCHAR(255) NOT NULL,
    file_url        VARCHAR(1024),
    reference_date  DATE,
    file_hash       BIGINT,
    last_updated_at TIMESTAMPTZ DEFAULT now(),
    created_at      TIMESTAMPTZ DEFAULT now()
);
```

### `ruc` (partitioned by status)

Main table holding all RUC records. Partitioned by `status` for query performance (partition pruning on status filters).

```sql
CREATE TABLE IF NOT EXISTS ruc (
    id               SERIAL,
    ruc              VARCHAR(20) NOT NULL,
    first_names      VARCHAR(255),
    last_names       VARCHAR(255),
    full_name        VARCHAR(512),
    check_digit      VARCHAR(5),
    old_ruc          VARCHAR(20),
    status           VARCHAR(50) NOT NULL,
    created_at       TIMESTAMPTZ DEFAULT now(),
    updated_at       TIMESTAMPTZ DEFAULT now(),
    file_metadata_id INTEGER REFERENCES ruc_file_metadata(id),
    PRIMARY KEY (id, status),
    UNIQUE (ruc, status)
) PARTITION BY LIST (status);
```

### Partitions

Create one partition per known status value, plus a default for any new statuses:

```sql
CREATE TABLE IF NOT EXISTS ruc_activo        PARTITION OF ruc FOR VALUES IN ('ACTIVO');
CREATE TABLE IF NOT EXISTS ruc_cancelado     PARTITION OF ruc FOR VALUES IN ('CANCELADO');
CREATE TABLE IF NOT EXISTS ruc_suspension    PARTITION OF ruc FOR VALUES IN ('SUSPENSION TEMPORAL');
CREATE TABLE IF NOT EXISTS ruc_bloqueado     PARTITION OF ruc FOR VALUES IN ('BLOQUEADO');
CREATE TABLE IF NOT EXISTS ruc_cancelado_def PARTITION OF ruc FOR VALUES IN ('CANCELADO DEFINITIVO');
CREATE TABLE IF NOT EXISTS ruc_default       DEFAULT PARTITION OF ruc;
```

## 4. Performance Indexes

GIN trigram indexes on `immutable_unaccent(full_name)` enable fast fuzzy search. These must be created on each **partition** individually (PostgreSQL does not support `CREATE INDEX CONCURRENTLY` on partitioned tables).

```sql
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_activo_fullname_unaccent_trgm
    ON ruc_activo USING gin (immutable_unaccent(full_name) gin_trgm_ops);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_cancelado_fullname_unaccent_trgm
    ON ruc_cancelado USING gin (immutable_unaccent(full_name) gin_trgm_ops);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_suspension_fullname_unaccent_trgm
    ON ruc_suspension USING gin (immutable_unaccent(full_name) gin_trgm_ops);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_bloqueado_fullname_unaccent_trgm
    ON ruc_bloqueado USING gin (immutable_unaccent(full_name) gin_trgm_ops);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_cancelado_def_fullname_unaccent_trgm
    ON ruc_cancelado_def USING gin (immutable_unaccent(full_name) gin_trgm_ops);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_default_fullname_unaccent_trgm
    ON ruc_default USING gin (immutable_unaccent(full_name) gin_trgm_ops);
```

> **Performance impact:** These indexes reduce fuzzy search time from ~3.8s to ~0.5s on a dataset of ~7M records.

## 5. Quick Setup (all-in-one)

Run the full setup in order:

```sql
-- Extensions
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS unaccent;

-- Immutable wrapper
CREATE OR REPLACE FUNCTION immutable_unaccent(text)
RETURNS text AS $$
    SELECT public.unaccent('public.unaccent', $1)
$$ LANGUAGE sql IMMUTABLE PARALLEL SAFE STRICT;

-- Tables
CREATE TABLE IF NOT EXISTS ruc_file_metadata (
    id              SERIAL PRIMARY KEY,
    file_name       VARCHAR(255) NOT NULL,
    file_url        VARCHAR(1024),
    reference_date  DATE,
    file_hash       BIGINT,
    last_updated_at TIMESTAMPTZ DEFAULT now(),
    created_at      TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE IF NOT EXISTS ruc (
    id               SERIAL,
    ruc              VARCHAR(20) NOT NULL,
    first_names      VARCHAR(255),
    last_names       VARCHAR(255),
    full_name        VARCHAR(512),
    check_digit      VARCHAR(5),
    old_ruc          VARCHAR(20),
    status           VARCHAR(50) NOT NULL,
    created_at       TIMESTAMPTZ DEFAULT now(),
    updated_at       TIMESTAMPTZ DEFAULT now(),
    file_metadata_id INTEGER REFERENCES ruc_file_metadata(id),
    PRIMARY KEY (id, status),
    UNIQUE (ruc, status)
) PARTITION BY LIST (status);

CREATE TABLE IF NOT EXISTS ruc_activo        PARTITION OF ruc FOR VALUES IN ('ACTIVO');
CREATE TABLE IF NOT EXISTS ruc_cancelado     PARTITION OF ruc FOR VALUES IN ('CANCELADO');
CREATE TABLE IF NOT EXISTS ruc_suspension    PARTITION OF ruc FOR VALUES IN ('SUSPENSION TEMPORAL');
CREATE TABLE IF NOT EXISTS ruc_bloqueado     PARTITION OF ruc FOR VALUES IN ('BLOQUEADO');
CREATE TABLE IF NOT EXISTS ruc_cancelado_def PARTITION OF ruc FOR VALUES IN ('CANCELADO DEFINITIVO');
CREATE TABLE IF NOT EXISTS ruc_default       DEFAULT PARTITION OF ruc;

-- GIN indexes (run one per partition)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_activo_fullname_unaccent_trgm
    ON ruc_activo USING gin (immutable_unaccent(full_name) gin_trgm_ops);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_cancelado_fullname_unaccent_trgm
    ON ruc_cancelado USING gin (immutable_unaccent(full_name) gin_trgm_ops);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_suspension_fullname_unaccent_trgm
    ON ruc_suspension USING gin (immutable_unaccent(full_name) gin_trgm_ops);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_bloqueado_fullname_unaccent_trgm
    ON ruc_bloqueado USING gin (immutable_unaccent(full_name) gin_trgm_ops);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_cancelado_def_fullname_unaccent_trgm
    ON ruc_cancelado_def USING gin (immutable_unaccent(full_name) gin_trgm_ops);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ruc_default_fullname_unaccent_trgm
    ON ruc_default USING gin (immutable_unaccent(full_name) gin_trgm_ops);
```

> **Note:** `CREATE INDEX CONCURRENTLY` cannot run inside a transaction. If using `psql`, execute each index statement separately or use a script without `BEGIN`/`COMMIT` wrapping.
