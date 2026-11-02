# Chatterbox Turbo runtime work in progress

Independent Rust implementation against the author's ONNX model contract:
https://huggingface.co/ResembleAI/chatterbox-turbo-ONNX/tree/d21799bd0354adb85e348b8a0442a8405110a2cf

This crate is not connected to the application's synthesis APIs or model catalog yet.
It currently verifies the pinned quantized bundle, reads the author's tokenizer
with its serialized postprocessor and runs text embedding and reference-voice conditioning networks on CPU.
A bounded generator now connects voice conditioning, sampled speech tokens with
owned KV caches, and waveform decoding. A reusable Engine retains the networks
and reference across narration chunks. Product integration, complete voice
preparation and process-level cancellation remain required.

`model.json` includes the model README, tokenizer, four quantized networks and
their external weights. Original author metadata is checked against pinned Git
blob or LFS identities. No Concat source code is included.

Run regular checks with the project toolchain:

```sh
CARGO_TARGET_DIR=backend/target/chatterbox cargo +1.89.0 test --manifest-path backend/chatterbox-runtime/Cargo.toml
```

The ignored learned test requires `VIDEO_KADR_TEST_CHATTERBOX_MODEL` pointing
to the complete verified bundle, `VIDEO_KADR_TEST_CHATTERBOX_REFERENCE` pointing
to a public mono24kHz PCM16 WAV (at least6seconds) for voice conditioning,
and `ORT_DYLIB_PATH` to a compatible local ONNX
Runtime library. Current qualification uses ONNX Runtime1.28.2/API27 on macOS
arm64. No runtime binary download, GPU provider or other platform is qualified.
The model is approximately1.12GB; this crate does not install it automatically.

A successful text embedding is not successful speech generation or voice cloning.

The `generation::synthesize` primitive accepts up to4096UTF-8 bytes,5..10seconds
of mono24kHz PCM and1..1024output tokens. It returns finite mono24kHz float PCM.
A stop token is required; hitting the limit returns an error instead of publishing
a truncated utterance. Default sampling uses temperature0.8, top-k1000, top-p0.95
and repetition penalty1.2; a supplied seed makes selection reproducible.
Cancellation is observed between synchronous ONNX calls. This is not yet the
application's managed/cancellable narration job path.

The ignored `actual_narration` test additionally needs
`VIDEO_KADR_TEST_CHATTERBOX_OUTPUT` naming an owned qualification WAV destination.
Current learned recording is public Bria from the previously verified Pocket
model. A recognized short utterance does not qualify voice similarity, complete
tag support, long-text quality or platform/performance parity.

`generation::synthesize_long` packs sentences toward250Unicode characters using
newlines and sentence-ending punctuation. Long sentences are kept whole rather
than silently truncated. Input is bounded to4096UTF-8 bytes and32chunks; total
audio is bounded to600seconds, with0.25seconds of silence between chunks.
Networks and reference preparation are reused, while speech history/KV caches
restart for each chunk. Progress identifies zero-based chunk and token counts.
A failed/cancelled chunk returns no partial successful audio.

Set `VIDEO_KADR_TEST_CHATTERBOX_LONG=1` for the optional two-chunk narration
qualification. Single-chunk control remains the default when the flag is absent.

The optional `actual_cancelled_engine_can_be_reused` test cancels a live read at
its first speech-token callback, requires cancellation rather than partial audio,
and performs a fresh read with the same Engine. It writes the successful second
read to `VIDEO_KADR_TEST_CHATTERBOX_OUTPUT`. Cancellation is checked immediately
after progress callbacks as well as between synchronous graph calls; the host
still needs process-level cancellation for interrupting an in-flight graph.

## Host process protocol

Build `--bin video-kadr-chatterbox` and run it with `--request /absolute/request.json`.
The request file is a bounded regular JSON file (32KiB maximum) with strict fields:

```json
{"schemaVersion":1,"modelDir":"/absolute/verified-model","reference":"/absolute/prepared-reference.wav","output":"/absolute/new-output.wav","text":"Welcome to our video project.","seed":42}
```

The reference must already be normalized into a canonical44byte-header mono24kHz
PCM16 WAV of5..10seconds; this protocol does not accept arbitrary user media.
The host owns source authorization, request-file permissions, process limits,
network/filesystem policy and lifecycle. Dynamic ONNX Runtime selection currently
uses `ORT_DYLIB_PATH`. No model/runtime downloading takes place in the child.

Progress JSON lines go to stderr. Successful stdout is a single JSON object with
schemaVersion, sampleRate, channels and frames. Narration is not placed in process
arguments or progress. Output is written through a private owned temporary file,
synced and published by no-replace hard link; existing output is never overwritten.
SIGTERM/default OS termination stops the process; the host must reap it and clean
its owned staging after abrupt termination. No host queue/API integration yet.

`scripts/qualify-chatterbox-cli.py` checks a real token1 termination/no partial WAV,
a fresh successful child, valid private PCM16 WAV, no replacement and unknown-field
refusal using explicit public-fixture/model/binary environment variables.

The request optionally accepts `runtimeLibrary` as an absolute host-selected ONNX
Runtime shared-library path. The application adapter uses this field because its
ProcessRuntime clears non-allowlisted environment variables; it does not inherit
`ORT_DYLIB_PATH`. The environment-based fallback remains for standalone developer
qualification. Neither field is a public HTTP parameter.
