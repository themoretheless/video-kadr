# Insta360 investigation, 2026-10-31

See ../../insta360-parity.md for product scope and limitations.

## Read-only source corpus

The user supplied a mounted media directory. 203 INSV directory entries were inventoried: 9 zero-byte entries, 193 recognized v3 metadata trailers, and 1 nonempty file without trailer magic. The independent model inventory reports X6 and four firmware strings. Do not extrapolate this to the user's other cameras. Private paths and raw footer samples remain in the local Codex artifact directory outside this repository. Originals were opened read-only. Full video hashes were not computed; metadata observations do not qualify video decode or calibration correctness.

## Observed v3 layout

- Last 72 bytes: reserved[32], extra extent uint32 LE, version uint32 LE (=3), ASCII magic[32] `8db42d694ccc418790edff439fe026bf`.
- Extra base = file length minus extra extent.
- Six bytes immediately before the footer identify the index: format byte, kind byte (both zero), payload length uint32 LE.
- Index rows are 10 bytes: kind byte, format byte, payload length uint32 LE, relative offset uint32 LE. Zero-length rows are empty. The nonempty kind equals its table slot in the supplied files.
- Block payload begins at extra base + relative offset. Its six-byte suffix has **format then kind**, opposite the first two index-row bytes. Length remains uint32 LE.
- Blocks can be separated by reserved padding. The index determines their offsets.
- Kind 1, format 1 carries ExtraMetadata protobuf. Only selected non-private fields are decoded by the product. Other indexed payloads remain opaque.

The first Rust corpus run exposed the index/suffix byte-order difference and failed closed. Its failed log is retained; the parser and synthetic fixture were corrected to include a non-palindromic kind/format pair.

## Static code / schema evidence

The REA Mach-O Evidence is referenced by feature-symbols.json. Additional embedded protobuf schema projections are in embedded-protobuf-descriptors.json, extra-metadata-descriptors.json and gyro-descriptors.json. These schema files were parsed independently from the same executable, and are not REA Evidence records. They describe declarations, not algorithm execution.

REA symbol addresses selected two GyroDataParser::ReadNext overloads. The LLVM supplement gyro-parser-llvm.asm confirms the 56-byte MotionItem and 20-byte RawGyroItem stride checks and copy operations. It does not establish unit conversion, axes, sensor fusion or frame synchronization. LLVM's Mach-O mode ignored the requested address bounds and produced a large full disassembly; the full result was moved outside the repository, retaining only those two functions here.

## REA rerun

```sh
GHIDRA_INSTALL_DIR=/Users/themoretheless/Applications/ghidra_12.1.4_PUBLIC \
REA_GHIDRA_STARTUP_TIMEOUT_MS=1800000 \
rea analyze '/Applications/Insta360 Studio.app' --provider ghidra \
  --snapshot docs/qualification/insta360-2026-10-31/analysis.snapshot.json --format json
```

The default native analysis hit its 1800-second provider deadline. The successful focused run is `focused-rea/run-vvewc9c9` (see `focused-run-4.log` and `analysis-summary.json`). It disables Decompiler Switch Analysis, Basic Constant Reference Analyzer and Objective-C Message Analyzer, and completed the whole-program overview and all 11 selected decompilations. It preserves 19 Evidence records and a provider-neutral snapshot. Earlier aborted/rejected profiles and diagnostics remain retained. No client configuration was changed. cli-unconfigured.json retains the rejected first CLI attempt before GHIDRA_INSTALL_DIR was supplied.

## Checks

Logs retain successful and failed attempts separately. Passing synthetic checks are distinct from the explicit local-footage parser check and the real Chromium + Rust preview test. See the parity document for exact commands and coverage.

The corrected native Rust metadata tests passed on 193 indexed trailers, with 9 zero-byte entries excluded and 1 nonempty file reporting no matching trailer. The final Chromium + real Rust test also covers the source metadata API and inspector. See metadata-real-final.log and browser-native-final.log.


## Successful static pass

The explicit reduced profile reports 133841 defined procedures, 288877 defined strings and 45 memory blocks. These are provider inventory counts, not feature counts. The main executable was imported; external libraries were not loaded. All 11 selected functions returned nonempty full-context pseudocode and Evidence. The source-bound function-only capsule run also completed 11/11; it carries separate raw-image identities and omits outside memory/ABI context. See capsules/manifest.json and capsule-run.log.

All owned REA sessions were exported and closed. The full 30-minute default-profile timeout, deliberate cancellation of long automatic analyses and the rejected analyzer-name attempt are distinct from the successful profile.
