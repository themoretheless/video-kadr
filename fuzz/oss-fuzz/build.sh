#!/bin/bash -eu

cd "$SRC/video-kadr/backend"
for target in edit_normalization multipart_path library_json url_policy cache_key; do
  cargo fuzz build "$target" --release
  cp "fuzz/target/x86_64-unknown-linux-gnu/release/$target" "$OUT/$target"
  if [[ -d "fuzz/corpus/$target" ]]; then
    zip -j "$OUT/${target}_seed_corpus.zip" "fuzz/corpus/$target"/*
  fi
done
