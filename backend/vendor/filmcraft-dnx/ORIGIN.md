Source: https://github.com/storytold/filmcraft/tree/5231852443363f001c3f6b396dd9b1e6461ae2be/crates/dnx

MIT source retained at the pinned commit. Cargo workspace inheritance is adapted to local path dependencies. The backend disables the optional DNx global rayon pool and schedules encoding through its existing export work. No external media fixtures are vendored.
