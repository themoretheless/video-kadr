//! Shared FFmpeg filter fragments used by edit and composition compilers.

pub mod atempo;

pub use atempo::atempo_filter_chain;
