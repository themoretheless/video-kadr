Source: https://github.com/jub0t/concat
Commit: 78e4be101c2d3c41a7dc73c112c6b1f11b9ce170

Original concat-text Rust renderer and runtime font assets, including font license notices. Cargo workspace inheritance is replaced by standalone package metadata; Rust 1.89 compatibility is qualified by the host integration gates. Runtime fonts are production dependencies, not test fixtures.

Host extensions in src/lib.rs: add_data and families_from_data load/inspect explicitly supplied project font bytes without opening source paths. Original rendering and bundled asset bytes remain unchanged.
