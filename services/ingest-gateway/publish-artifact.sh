#!/bin/sh
set -eu

stream_name="$1"
segment_path="$2"
duration="$3"
case "$stream_name" in
  ''|*[!A-Za-z0-9._-]*) exit 64 ;;
esac
artifact_id="$(basename "$segment_path" | tr -cd 'A-Za-z0-9._-')"
artifact_dir="/var/lib/ingest/artifacts/$stream_name"
artifact_path="$artifact_dir/$artifact_id"
manifest_tmp="$artifact_path.json.part"
manifest_path="$artifact_path.json"

mkdir -p "$artifact_dir"
# MediaMTX calls this hook only after the segment is finalized. A hard link
# freezes the exact inode handed to the editor without copying live bytes.
ln "$segment_path" "$artifact_path"
sha256="$(sha256sum "$artifact_path" | cut -d ' ' -f 1)"
byte_length="$(wc -c < "$artifact_path" | tr -d ' ')"
printf '{"schemaVersion":1,"artifactId":"%s","path":"%s","byteLength":%s,"sha256":"%s","duration":"%s"}\n' \
  "$artifact_id" "$artifact_path" "$byte_length" "$sha256" "$duration" > "$manifest_tmp"
mv "$manifest_tmp" "$manifest_path"
