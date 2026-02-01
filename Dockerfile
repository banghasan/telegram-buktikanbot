# syntax=docker/dockerfile:1.5
FROM rust:1-slim-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
# Create dummy main.rs to build dependencies first (caching layer)
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release

# Copy actual source code
COPY src ./src
# Touch main.rs to ensure cargo rebuilds the binary with new source
RUN touch src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp /app/target/release/telegram-buktikanbot /app/telegram-buktikanbot

FROM debian:bookworm-slim
# Install required runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    tzdata \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -u 10001 appuser
COPY --from=builder /app/telegram-buktikanbot /usr/local/bin/telegram-buktikanbot
USER appuser
ENTRYPOINT ["/usr/local/bin/telegram-buktikanbot"]
