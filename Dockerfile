# =============================================================================
# Axpert Multi-Inverter Monitor — Docker image
#
# Multi-stage build:
#   Stage 1 (builder): Rust on Alpine — compiles a fully static (musl) binary
#   Stage 2 (runtime): Alpine — binary + web UI + templates + admin tools
#
# The image is self-contained: the binary is statically linked (TLS via
# rustls, no OpenSSL) and the web UI is served by the binary itself. The only
# external service it talks to is the MQTT broker.
#
# Volumes (map via docker-compose):
#   /app/config  ← persistent configuration directory
#   /app/data    ← persistent redb database directory
# =============================================================================

# ── Stage 1: Build ────────────────────────────────────────────────────────────
FROM rust:1.90-alpine AS builder

# C toolchain for crates with native code (ring, used by rustls)
RUN apk add --no-cache build-base

WORKDIR /build

# ── Cache Cargo dependencies ──────────────────────────────────────────────────
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && \
    echo 'fn main() {}' > src/main.rs && \
    cargo build --release --locked && \
    rm -f target/release/axpert-monitor* target/release/deps/axpert_monitor*

# ── Build the real application ────────────────────────────────────────────────
COPY src ./src
RUN cargo build --release --locked && \
    # Fail the build if the binary is not fully static
    ! ldd target/release/axpert-monitor 2>/dev/null | grep -q '=>'

# ── Stage 2: Runtime ──────────────────────────────────────────────────────────
FROM alpine:3.22

ARG VERSION=dev
LABEL org.opencontainers.image.title="Axpert Multi-Inverter Monitor" \
      org.opencontainers.image.description="Monitor for Voltronic Axpert inverters: web UI, MQTT and Home Assistant discovery" \
      org.opencontainers.image.version="${VERSION}" \
      org.opencontainers.image.licenses="MIT"

# Runtime essentials:
#   tini           init process (signal handling, zombie reaping)
#   tzdata         timezones (daily energy counters reset at local midnight)
#   ca-certificates
# Admin / troubleshooting tools:
#   sudo nano bash less curl jq htop procps
#   iproute2 iputils bind-tools   network: ip, ping, dig
#   netcat-openbsd                 test the inverters' TCP adapters (nc -vz IP PORT)
#   mosquitto-clients              test the broker (mosquitto_sub -h IP -t 'axpert/#' -v)
RUN apk add --no-cache \
        tini tzdata ca-certificates \
        sudo nano bash less curl jq htop procps \
        iproute2 iputils bind-tools netcat-openbsd mosquitto-clients

# "apt" compatibility helper (Alpine uses apk)
COPY docker/apt /usr/local/bin/apt
RUN chmod +x /usr/local/bin/apt

WORKDIR /app

COPY --from=builder /build/target/release/axpert-monitor ./axpert-monitor
COPY www ./www
COPY config.defaults ./config.defaults
COPY entrypoint.sh ./entrypoint.sh
RUN chmod +x entrypoint.sh && mkdir -p config data

ENV TZ=Europe/Madrid

# Web UI port
EXPOSE 60606

VOLUME ["/app/config", "/app/data"]

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://127.0.0.1:60606/api/health 2>/dev/null | grep -q ok || exit 1

ENTRYPOINT ["/sbin/tini", "--", "/app/entrypoint.sh"]
