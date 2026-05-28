# Build stage: Alpine-based musl Rust toolchain.
FROM rust:1.83-alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev pkgconfig

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests
COPY systemd ./systemd

# Build the binary statically against musl. The resulting binary
# has no glibc dependency and runs on any modern Linux.
RUN cargo build --release --target x86_64-unknown-linux-musl && \
    strip target/x86_64-unknown-linux-musl/release/tg

# Runtime stage: minimal Alpine. tmux + ffmpeg are runtime deps for
# the daemon path; without them tg listen will fail at runtime (but
# tg send works without ffmpeg, and tg send/init/install work
# without tmux).
FROM alpine:3.20

RUN apk add --no-cache tmux ffmpeg ca-certificates

# Non-root user; mirrors the systemd --user pattern.
RUN adduser -D -h /home/tg tg
USER tg
WORKDIR /home/tg

COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/tg /usr/local/bin/tg

# Bind-mount points the operator wires up at run time:
#   /home/tg/.tg     — config + state (token-bearing; chmod 0700 outside)
#
# Example:
#   docker run -d --name tg-listen \
#       -v "$HOME/.tg:/home/tg/.tg" \
#       -e TG_TARGET_PANE_HOST=host.docker.internal \
#       ghcr.io/devpermutations/tg:latest tg listen
#
# Note: the daemon's send-keys needs to reach a tmux pane on the host;
# you'll either need --network host plus a tmux socket bind-mount, or
# run the daemon directly on the host. Containerised tg listen is
# mostly useful for testing the build, not for production single-host
# deployment.

ENTRYPOINT ["/usr/local/bin/tg"]
CMD ["--help"]
