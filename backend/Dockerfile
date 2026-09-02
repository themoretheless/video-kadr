# Build stage
FROM rust:1-bookworm AS build
WORKDIR /app
COPY . .
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim
ARG YT_DLP_VERSION=2026.08.19
RUN apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg ca-certificates curl python3 \
    && curl -fsSLo /tmp/yt-dlp "https://github.com/yt-dlp/yt-dlp/releases/download/${YT_DLP_VERSION}/yt-dlp" \
    && curl -fsSLo /tmp/SHA2-256SUMS "https://github.com/yt-dlp/yt-dlp/releases/download/${YT_DLP_VERSION}/SHA2-256SUMS" \
    && grep ' yt-dlp$' /tmp/SHA2-256SUMS | sed 's# yt-dlp$# /tmp/yt-dlp#' | sha256sum -c - \
    && mv /tmp/yt-dlp /usr/local/bin/yt-dlp \
    && chmod a+rx /usr/local/bin/yt-dlp \
    && rm -f /tmp/SHA2-256SUMS \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/video-kadr-backend /usr/local/bin/video-kadr-backend
RUN groupadd --gid 10001 video-kadr \
    && useradd --uid 10001 --gid 10001 --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin video-kadr \
    && mkdir -p /data \
    && chown video-kadr:video-kadr /data
ENV STORAGE_DIR=/data
# Localhost by default; docker-compose may override this for an internal proxy.
ENV BIND_ADDR=127.0.0.1
VOLUME /data
EXPOSE 8080
USER 10001:10001
CMD ["video-kadr-backend"]
