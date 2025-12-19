#![allow(clippy::needless_doctest_main)]
#![doc = include_str!("../README.md")]

mod diff;
mod patch;
mod bsdf2;

pub use diff::diff;
pub use patch::patch;
pub use bsdf2::{patch_bsdf2, CompressionAlgorithm};

pub use patch::patch as apply_patch;
pub use bsdf2::patch_bsdf2 as apply_bsdf2_patch;