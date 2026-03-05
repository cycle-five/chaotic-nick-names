# ── Build stage ────────────────────────────────────────────────────────────────
FROM rust:1.76-slim AS builder

WORKDIR /app

# Cache dependency compilation separately from application code
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src

# Build the real application
COPY src ./src
COPY migrations ./migrations
RUN touch src/main.rs && cargo build --release

# ── Runtime stage ──────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/chaotic-nick-names ./chaotic-nick-names

ENV RUST_LOG=chaotic_nick_names=info,warn

CMD ["./chaotic-nick-names"]
