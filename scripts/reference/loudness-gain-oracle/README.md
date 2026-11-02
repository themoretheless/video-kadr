Build and explicitly run the independent Loudnorm gain test:

```sh
python3 scripts/reference/loudness-gain-oracle/build.py /path/to/af_loudnorm.c /tmp/loudness-gain-oracle
LOUDNESS_GAIN_ORACLE=/tmp/loudness-gain-oracle CARGO_TARGET_DIR=backend/target cargo +1.89.0 test --manifest-path backend/Cargo.toml --lib domain::concat_loudness_gain::tests -- --include-ignored --nocapture
```

The builder requires the exact SHA-256 recorded in its source. It extracts the
original Gaussian functions and gain-history update block from FFmpeg's
`libavfilter/af_loudnorm.c`. The only replaced meter call supplies a test metric;
Gaussian arithmetic and update expressions compile directly as original C.
Default Concat settings are target LRA=11 and measured threshold=-70. The test
covers the preset's target endpoints/default and silent/threshold/audible initial
states over eight full history wraps. No large numeric fixture is stored.

Algorithm copyright 2016 Kyle Swanson; LGPL-2.1-or-later. License text:
`backend/licenses/FFmpeg-LGPL-2.1.txt`. No C oracle is linked into the product.
