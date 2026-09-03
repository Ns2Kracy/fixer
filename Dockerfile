FROM node:24.15.0-bookworm-slim AS web-build
WORKDIR /build/web
RUN corepack enable && corepack prepare pnpm@11.22.0 --activate
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

FROM rust:1.85-bookworm AS server-build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --locked --release -p fixer-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl tini \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 fixer \
    && useradd --uid 10001 --gid fixer --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin fixer \
    && mkdir -p /app/web/dist /data /media \
    && chown -R fixer:fixer /data /media
WORKDIR /app
COPY --from=server-build /build/target/release/fixer-server /usr/local/bin/fixer-server
COPY --from=web-build /build/web/dist/ /app/web/dist/
COPY fixer.toml.example /usr/share/fixer/fixer.toml.example
COPY --chmod=0755 scripts/docker-entrypoint.sh /usr/local/bin/docker-entrypoint
ENV FIXER_CONFIG=/data/fixer.toml \
    FIXER_SERVER__BIND=0.0.0.0:3000 \
    FIXER_SERVER__DATABASE=/data/fixer.sqlite3 \
    FIXER_SERVER__MEDIA_ROOTS=/media \
    FIXER_SERVER__WEB_ROOT=/app/web/dist
EXPOSE 3000
VOLUME ["/data"]
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=5 \
    CMD ["curl", "--fail", "--silent", "--show-error", "http://127.0.0.1:3000/api/v1/health"]
STOPSIGNAL SIGINT
USER 10001:10001
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/docker-entrypoint"]
CMD ["fixer-server"]
