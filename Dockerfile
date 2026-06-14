FROM rust:1.91-bookworm AS builder

RUN apt-get update && apt-get install -y \
    cmake \
    g++ \
    pkg-config \
    libssl-dev \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src src/bin \
 && echo 'fn main() {}' > src/main.rs \
 && echo 'fn main() {}' > src/bin/migrate.rs \
 && echo '' > src/lib.rs \
 && cargo build --release --bin server --bin migrate || true \
 && rm -rf src

COPY . .
RUN cargo build --release --bin server --bin migrate

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    libssl3 \
    ca-certificates \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /src/target/release/server /app/server
COPY --from=builder /src/target/release/migrate /app/migrate
RUN chmod +x /app/server /app/migrate
ENTRYPOINT ["/app/server"]
