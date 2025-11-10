FROM rust:1.91 AS builder
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release --manifest-path backend/ai-service/Cargo.toml --bin ai-service
RUN ls -l target/release/
RUN strip target/release/ai-service

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/ai-service /usr/local/bin/ai-service
EXPOSE 9000
HEALTHCHECK --interval=10s --timeout=3s --retries=5 CMD curl -f http://localhost:9000/ || exit 1
CMD ["/usr/local/bin/ai-service"]
