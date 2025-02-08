FROM lukemathwalker/cargo-chef:latest-rust-alpine AS base
RUN apk add --no-cache musl-dev sccache
ENV RUSTC_WRAPPER=sccache SCCACHE_DIR=/sccache
WORKDIR /app

FROM base AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
COPY --from=planner /app/recipe.json ./
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo build --release

FROM scratch
LABEL org.opencontainers.image.description="DnBRadio Discord and IRC bot"
COPY --from=builder /app/target/release/dnbradio-bot /dnbradio-bot
USER 1000
CMD ["/dnbradio-bot"]
