FROM rust:1.90-slim-bookworm AS builder
WORKDIR /app
 
# Cache deps: build against a stub main first
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src target/release/deps/liftoff*
 
COPY src ./src
RUN cargo build --release
 
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/liftoff /usr/local/bin/liftoff
ENV RUST_LOG=info
CMD ["liftoff"]