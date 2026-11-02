Independent reference for Loudnorm's 192 kHz limiter:

```sh
python3 scripts/reference/loudness-limiter-oracle/build.py /path/to/af_loudnorm.c /tmp/loudness-limiter-oracle
LOUDNESS_LIMITER_ORACLE=/tmp/loudness-limiter-oracle CARGO_TARGET_DIR=backend/target cargo +1.89.0 test --manifest-path backend/Cargo.toml --lib domain::concat_loudness_limiter::tests -- --include-ignored --nocapture
```

The SHA-256 guarded builder extracts `detect_peak` and `true_peak_limiter`
unchanged from FFmpeg's `libavfilter/af_loudnorm.c`. The tiny harness supplies a
192 kHz, 210 ms ring and retains original envelope state between calls. Each test
call supplies its ring and write index, then compares output and the mutated
ring. Comparison includes sparse impulses, dense alternating bursts, initial
ceiling excess and below-ceiling sine, mono/stereo, ring wraps and short blocks.

These component inputs exercise internal state and are not full normalizer
streaming proof. Production lookahead and EOF scheduling need separate tests.
No C oracle is linked into the product and no numeric fixture is committed.

Original algorithm copyright 2016 Kyle Swanson; LGPL-2.1-or-later.
License: `backend/licenses/FFmpeg-LGPL-2.1.txt`. No warranty.
