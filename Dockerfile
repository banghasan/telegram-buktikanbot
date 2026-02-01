FROM debian:bookworm-slim
# syntax=docker/dockerfile:1.5
ARG VERSION
ARG TARGETARCH

# Install required runtime dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    tzdata \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -u 10001 appuser
RUN set -eux; \
    case "${TARGETARCH}" in \
      amd64) target="x86_64-unknown-linux-gnu" ;; \
      arm64) target="aarch64-unknown-linux-gnu" ;; \
      *) echo "unsupported arch: ${TARGETARCH}" >&2; exit 1 ;; \
    esac; \
    asset="buktikanbot-${VERSION}-${target}.tar.gz"; \
    url="https://github.com/banghasan/telegram-buktikanbot/releases/download/v${VERSION}/${asset}"; \
    curl -fL "$url" -o /tmp/bot.tar.gz; \
    tar -xzf /tmp/bot.tar.gz -C /tmp; \
    install -m 0755 /tmp/buktikanbot /usr/local/bin/telegram-buktikanbot; \
    rm -f /tmp/bot.tar.gz /tmp/buktikanbot /tmp/README.md
USER appuser
ENTRYPOINT ["/usr/local/bin/telegram-buktikanbot"]
