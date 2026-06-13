#!/usr/bin/env bash
# Run backend (cargo) and frontend (vite) together for local development.
# Ctrl-C stops both.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "Ошибка: ffmpeg не найден в PATH. Установи: brew install ffmpeg" >&2
  exit 1
fi
if ! command -v yt-dlp >/dev/null 2>&1; then
  echo "Ошибка: yt-dlp не найден в PATH. Установи: brew install yt-dlp" >&2
  exit 1
fi

backend_pid=""
cleanup() {
  if [ -n "$backend_pid" ] && kill -0 "$backend_pid" 2>/dev/null; then
    kill "$backend_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

echo "Запускаю бэкенд (cargo run) на :8080 ..."
(cd "$ROOT/backend" && cargo run) &
backend_pid=$!

echo "Запускаю фронтенд (npm run dev) на :5173 ..."
cd "$ROOT/frontend"
npm run dev
