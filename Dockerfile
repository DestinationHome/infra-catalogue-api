# Syntax=docker/dockerfile:1

# Multi-arch base builder with pre-installed cargo-chef
FROM lukemathwalker/cargo-chef:latest-rust-1-alpine AS chef
WORKDIR /app
RUN apk add --no-cache ca-certificates musl-dev

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build & cache dependencies for native host architecture (x86_64 / aarch64)
RUN cargo chef cook --release --recipe-path recipe.json

# Copy source code and build native release binary statically linked against musl
COPY . .
RUN cargo build --release --bin catalogue

# Final minimal stage: scratch (0 bytes overhead, zero OS attack surface)
FROM scratch AS runtime

# Copy SSL CA root certificates for HTTPS/TLS client calls (reqwest, Meilisearch, MongoDB)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

# Copy statically linked catalogue binary
COPY --from=builder /app/target/release/catalogue /catalogue

EXPOSE 8080
ENTRYPOINT ["/catalogue"]
