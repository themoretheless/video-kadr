Pinned source: https://github.com/storytold/filmcraft commit 5231852443363f001c3f6b396dd9b1e6461ae2be, crates/captions.

Native file codec library: original SRT, WebVTT, SCC, MCC, EBU STL, TTML and DFXP readers/writers, original unit and property tests. File parsing/writing and exact original Tick storage remain unchanged.

Local adaptations: standalone Cargo manifest targets Rust 1.89 and existing vendored filmcraft-time; project plain_text is copied verbatim into this crate; CaptionFormat classification is local; project-model conversion functions and burn module are omitted because video-kadr owns those adapters and rendering separately. SCC/MCC plain_text calls point to the local identical helper. No claim of editor import/export parity follows from codec tests.
