#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
port="${SCENE_DESCRIPTION_OLLAMA_PORT:-11534}"
models="${SCENE_DESCRIPTION_MODEL_DIR:-$ROOT/backend/storage/models/scene-description}"

if [ "$#" -eq 0 ]; then
  echo "Использование: $0 команда [аргументы]" >&2
  exit 2
fi
if [ -z "${SCENE_DESCRIPTION_INSTALLER:-}" ]; then
  echo "Задайте SCENE_DESCRIPTION_INSTALLER с именем учётной записи Video Kadr." >&2
  exit 2
fi
if [ -n "${SCENE_DESCRIPTION_ENDPOINT:-}" ] || [ -n "${SCENE_DESCRIPTION_OLLAMA_URL:-}" ]; then
  echo "Для совместного запуска уберите SCENE_DESCRIPTION_ENDPOINT и SCENE_DESCRIPTION_OLLAMA_URL." >&2
  exit 2
fi
if [[ ! "$port" =~ ^[1-9][0-9]{0,4}$ ]] || [ "$port" -gt 65535 ]; then
  echo "Недопустимый SCENE_DESCRIPTION_OLLAMA_PORT." >&2
  exit 2
fi
for tool in curl ollama; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Для запуска локальной модели требуется $tool в PATH." >&2
    exit 1
  fi
done
if (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
  echo "Порт Ollama $port уже занят. Укажите другой SCENE_DESCRIPTION_OLLAMA_PORT." >&2
  exit 1
fi

mkdir -p "$models"
models="$(cd "$models" && pwd -P)"
export SCENE_DESCRIPTION_OLLAMA_URL="http://127.0.0.1:$port"

# Own two process groups so a development launcher can also start children.
set -m
app_pid=""
ollama_pid=""

stop_group() {
  local pid="$1" timeout="$2" elapsed
  if [ -z "$pid" ]; then return; fi
  kill -TERM -- "-$pid" 2>/dev/null || true
  for ((elapsed = 0; elapsed < timeout * 4; elapsed++)); do
    if ! kill -0 -- "-$pid" 2>/dev/null; then break; fi
    sleep 0.25
  done
  if kill -0 -- "-$pid" 2>/dev/null; then
    kill -KILL -- "-$pid" 2>/dev/null || true
  fi
  wait "$pid" 2>/dev/null || true
}

cleanup() {
  trap '' INT TERM HUP
  stop_group "$app_pid" 65
  stop_group "$ollama_pid" 10
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

OLLAMA_HOST="127.0.0.1:$port" OLLAMA_MODELS="$models" OLLAMA_NO_CLOUD=1 \
  OLLAMA_CONTEXT_LENGTH="${OLLAMA_CONTEXT_LENGTH:-8192}" OLLAMA_NUM_PARALLEL="${OLLAMA_NUM_PARALLEL:-1}" \
  ollama serve &
ollama_pid=$!

ready=false
for ((attempt = 0; attempt < 40; attempt++)); do
  sleep 0.25
  if ! kill -0 "$ollama_pid" 2>/dev/null; then
    echo "Ollama завершился при запуске." >&2
    exit 1
  fi
  if curl --noproxy '*' -fsS --max-time 1 "$SCENE_DESCRIPTION_OLLAMA_URL/api/tags" >/dev/null 2>&1; then
    ready=true
    break
  fi
done
if [ "$ready" = false ]; then
  echo "Ollama не ответил по адресу $SCENE_DESCRIPTION_OLLAMA_URL." >&2
  exit 1
fi

"$@" &
app_pid=$!
while kill -0 "$app_pid" 2>/dev/null; do
  if ! kill -0 "$ollama_pid" 2>/dev/null; then
    echo "Служба Ollama завершилась." >&2
    exit 1
  fi
  sleep 0.25
done

status=0
wait "$app_pid" || status=$?
exit "$status"
