FROM rust:1.91 AS builder
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release --manifest-path backend/api-gateway/Cargo.toml --bin api-gateway
RUN strip target/release/api-gateway

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/api-gateway /usr/local/bin/api-gateway
EXPOSE 8000
HEALTHCHECK --interval=10s --timeout=3s --retries=5 CMD curl -f http://localhost:8000/ || exit 1
CMD ["/usr/local/bin/api-gateway"]
