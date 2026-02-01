FROM rust:1.91-slim AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release

COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN useradd -r -u 10001 appuser
COPY --from=builder /app/target/release/telegram-buktikanbot /usr/local/bin/telegram-buktikanbot
USER appuser
ENTRYPOINT ["/usr/local/bin/telegram-buktikanbot"]
