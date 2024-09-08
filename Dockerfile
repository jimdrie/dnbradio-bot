FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app
COPY Cargo.toml ./
RUN mkdir .cargo && cargo vendor > .cargo/config.toml
RUN echo "fn main() {}" > dummy.rs && \
    sed -i 's/src\/main.rs/dummy.rs/g' Cargo.toml && \
    cargo build --release && \
    rm dummy.rs && \
    sed -i 's/dummy.rs/src\/main.rs/g' Cargo.toml && \
    rm -rf target/release/.fingerprint/dnbradio-bot-*
COPY src src
RUN cargo build --release

FROM scratch
COPY --from=builder /app/target/release/dnbradio-bot /dnbradio-bot
USER 1000
CMD ["/dnbradio-bot"]
