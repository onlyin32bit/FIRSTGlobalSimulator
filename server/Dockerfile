FROM rust:1.89-bookworm AS builder

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src \
    && printf 'fn main() {}\n' > src/main.rs \
    && cargo build --release --locked \
    && rm -rf src

COPY src ./src
RUN cargo build --release --locked \
    && strip target/release/server

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --no-install-recommends --yes ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 fgsim

COPY --from=builder /app/target/release/server /usr/local/bin/fgsim-server

ENV RUST_LOG=info

USER fgsim
WORKDIR /home/fgsim
EXPOSE 3000

ENTRYPOINT ["/usr/local/bin/fgsim-server"]
