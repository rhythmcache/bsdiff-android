#![allow(clippy::needless_doctest_main)]
#![doc = include_str!("../README.md")]

mod diff;
mod patch;
mod bsdf2;
mod bsdf2_writer;

pub use diff::{diff, diff_bsdiff40, diff_bsdf2, diff_bsdf2_uniform};
pub use patch::patch;
pub use bsdf2::{patch_bsdf2, parse_bsdf2_header};

pub use bsdf2_writer::{CompressionAlgorithm, ControlEntry, Bsdf2Writer};

pub use patch::patch as apply_patch;
pub use bsdf2::patch_bsdf2 as apply_bsdf2_patch;
