FROM rust:1.90-slim AS builder
WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml ./
COPY Cargo.lock* ./
COPY src ./src

RUN cargo build --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/target/release/sendmail-mcp /app/sendmail-mcp

EXPOSE 8080
ENV MCP_BIND=0.0.0.0:8080
ENV CONFIG_PATH=/app/config.toml

ENTRYPOINT ["/app/sendmail-mcp"]
