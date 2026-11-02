# Native parade browser fixture

`native-parade.json` is the `parade` object captured from the real `/api/compositions/scopes` response by `native_scopes_reports_levels_graphs_and_black_composited_transparency` in `backend/tests/api.rs`.

Regenerate from the repository root:

```sh
VIDEO_KADR_SCOPE_BROWSER_FIXTURE=/tmp/video-kadr-native-parade-browser.json cargo test --manifest-path backend/Cargo.toml --test api native_scopes_reports_levels_graphs_and_black_composited_transparency
cp /tmp/video-kadr-native-parade-browser.json frontend/e2e/fixtures/native-parade.json
```

The capture input is an opaque 320×180 eight-color-bar raster, RGB parade with 64 level rows. Production native analysis/painting supplies pixels, axes and divider geometry; the browser harness mounts the production `NativeScopeGraph` component. This checks Chromium Canvas/SVG rendering at wide and narrow widths. It does not exercise authenticated video decoding, the complete composition workspace, HDR, or other browser engines.
