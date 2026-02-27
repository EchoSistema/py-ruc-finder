# Stage 1: Build
FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Cache dependencies — only re-downloaded when Cargo files change
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

# Build the actual application
COPY src/ src/
RUN touch src/main.rs && cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/ruc_finder /usr/local/bin/ruc_finder
COPY ruc_finder.conf /etc/ruc_finder/ruc_finder.conf

EXPOSE 3000

ENTRYPOINT ["ruc_finder"]
