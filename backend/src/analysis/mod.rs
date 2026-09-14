//! Derived media analysis artifacts. Source media remains authoritative.

pub mod cropdetect;
pub mod proxy;
#[cfg(feature = "spec-contracts")]
pub mod quality;
pub mod thumbnail;
