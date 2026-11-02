Source: https://github.com/storytold/filmcraft/tree/5231852443363f001c3f6b396dd9b1e6461ae2be/crates/isobmff

Pinned MIT source; workspace Cargo declarations adapted to local paths. No external media fixtures are copied.

Local change: src/mux.rs additionally writes named UTF-8 metadata as a standard mdta meta/keys/ilst block. Original QuickTime copyright tags are retained. The native export adapter uses this to expose description to external readers alongside the original ©des tag.
