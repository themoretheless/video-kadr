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
COPY --from=build /app/target/release/video-editor-backend /usr/local/bin/video-editor-backend
ENV STORAGE_DIR=/data
# Bind to all interfaces so the container is reachable (server defaults to 127.0.0.1).
ENV BIND_ADDR=0.0.0.0
VOLUME /data
EXPOSE 8080
CMD ["video-editor-backend"]
