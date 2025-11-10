FROM rust:1.91 AS builder
WORKDIR /usr/src/app
COPY . .
RUN cargo build --release --manifest-path backend/scraper/Cargo.toml --bin scraper && strip target/release/scraper

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/scraper /usr/local/bin/scraper
HEALTHCHECK --interval=10s --timeout=3s --retries=5 CMD ["/usr/local/bin/scraper", "--healthcheck"]
CMD ["/usr/local/bin/scraper"]
