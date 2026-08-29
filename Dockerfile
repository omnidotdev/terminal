# Build the WASM frontend and web-server binary, then package into a minimal image.
#
# Usage:
#   docker build -t omni-terminal-web .
#   docker run -p 3000:3000 omni-terminal-web
#
# Environment variables:
#   PORT       - Server listen port (default: 3000)
#   TLS_CERT   - Path to TLS certificate PEM file (auto-generated if unset)
#   TLS_KEY    - Path to TLS private key PEM file (auto-generated if unset)

FROM rust:1.93-bookworm@sha256:7c4ae649a84014c467d79319bbf17ce2632ae8b8be123ac2fb2ea5be46823f31 AS builder

RUN rustup target add wasm32-unknown-unknown \
    && cargo install wasm-bindgen-cli --version 0.2.106

WORKDIR /build
COPY . .

# Build WASM frontend
RUN cargo build -p omni-terminal-wasm --target wasm32-unknown-unknown --release \
    && wasm-bindgen target/wasm32-unknown-unknown/release/omni_terminal_wasm.wasm \
       --out-dir frontends/wasm/wasm --target web --no-typescript

# Build web-server binary
RUN cargo build -p web-server --release

# Runtime image
FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/web-server /srv/web-server
COPY --from=builder /build/frontends/wasm/index.html /srv/frontends/wasm/index.html
COPY --from=builder /build/frontends/wasm/wasm/ /srv/frontends/wasm/wasm/

WORKDIR /srv
ENV PORT=3000
EXPOSE 3000

CMD ["/srv/web-server"]
