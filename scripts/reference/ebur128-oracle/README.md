Build an independent reference meter from the exact audited FFmpeg source bytes:

```sh
python3 scripts/reference/ebur128-oracle/build.py /path/to/ebur128.c /path/to/ebur128.h /tmp/ebu-meter
EBU128_ORACLE=/tmp/ebu-meter CARGO_TARGET_DIR=backend/target cargo +1.89.0 test --manifest-path backend/Cargo.toml --lib domain::concat_ebur128::tests -- --include-ignored --nocapture
```

The builder checks SHA-256 before compiling; it never rewrites the source. Inputs
come from FFmpeg `libavfilter/ebur128.c` and `ebur128.h`; the accepted hashes are
recorded in `build.py`. The shims replace allocation, alignment, error constants
and one-time initialization only. The standalone oracle is single-threaded.
No C reference code is linked into the product. The original notices are in
`backend/licenses/FFmpeg-ebur128-NOTICE.txt` and the LGPL text is adjacent.

The test generates PCM at runtime and sends it as packed doubles to the oracle.
It compares integrated/short-term loudness, relative threshold, LRA and each
channel's peak for the same packet sizes. No large numeric fixture is stored.
The test is explicitly ignored by default because it requires the external
independent executable; a normal green test run does not imply oracle parity.
