# Build stage
FROM rust:1.85-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock* ./
COPY migrations ./migrations
COPY src ./src

RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/erp_rust /usr/local/bin/erp_rust
COPY templates ./templates
COPY migrations ./migrations

RUN mkdir -p /app/media /app/static /app/private

ENV HOST=0.0.0.0
ENV PORT=8003
EXPOSE 8003

CMD ["erp_rust"]

