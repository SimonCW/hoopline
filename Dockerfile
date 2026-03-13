FROM rust:1-slim-bookworm AS chef
WORKDIR /app

RUN cargo install cargo-chef --locked

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --locked --recipe-path recipe.json
COPY . .
RUN cargo build --release --locked --bin hoopline

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --uid 10001 appuser
WORKDIR /app

COPY --from=builder /app/target/release/hoopline /usr/local/bin/hoopline

ENV DATABASE_URL=sqlite:///data/hoopline.db

USER appuser
EXPOSE 3000

CMD ["hoopline"]
