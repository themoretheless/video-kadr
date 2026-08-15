# Build stage
FROM rust:1-bookworm AS build
WORKDIR /app
COPY . .
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg ca-certificates curl python3 \
    && curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp -o /usr/local/bin/yt-dlp \
    && chmod a+rx /usr/local/bin/yt-dlp \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/video-kadr-backend /usr/local/bin/video-kadr-backend
ENV STORAGE_DIR=/data
# Localhost by default; docker-compose may override this for an internal proxy.
ENV BIND_ADDR=127.0.0.1
VOLUME /data
EXPOSE 8080
CMD ["video-kadr-backend"]
