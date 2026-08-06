# syntax=docker/dockerfile:1

FROM lukemathwalker/cargo-chef:latest-rust-1-bookworm AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin bigdogcal-cli

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/bigdogcal-cli /app/bigdogcal-cli
COPY config /app/config
COPY assets /app/assets

ENV LOCO_ENV=production

EXPOSE 5150

ENTRYPOINT ["/app/bigdogcal-cli"]
CMD ["start"]
