# syntax=docker/dockerfile:1

FROM rust:1.98.1-alpine3.22 AS builder

RUN apk add --no-cache musl-dev

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --locked --release -p gitbox -p gitboxd

FROM alpine:3.22

RUN apk add --no-cache git openssh-server su-exec \
    && addgroup -S -g 10001 gitbox \
    && adduser -S -D -h /data -s /bin/sh -G gitbox -u 10001 gitbox \
    && passwd -d gitbox \
    && install -d -m 0700 /run/sshd

COPY --from=builder /build/target/release/gitbox /usr/local/bin/gitbox
COPY --from=builder /build/target/release/gitboxd /usr/local/bin/gitboxd
COPY docker/entrypoint.sh /usr/local/bin/gitbox-entrypoint
COPY docker/sshd_config /etc/ssh/sshd_config

RUN chmod +x /usr/local/bin/gitbox-entrypoint

EXPOSE 22
ENTRYPOINT ["/usr/local/bin/gitbox-entrypoint"]
