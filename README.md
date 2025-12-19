# bsdiff-android

[![GitHub](https://img.shields.io/badge/github-bsdiff--android-8da0cb?logo=github)](https://github.com/YOUR_USERNAME/bsdiff-android)
[![crates.io version](https://img.shields.io/crates/v/bsdiff-android.svg)](https://crates.io/crates/bsdiff-android)
[![docs.rs docs](https://docs.rs/bsdiff-android/badge.svg)](https://docs.rs/bsdiff-android)
[![CI build](https://github.com/YOUR_USERNAME/bsdiff-android/actions/workflows/rust.yml/badge.svg)](https://github.com/YOUR_USERNAME/bsdiff-android/actions)

Bsdiff/bspatch implementation with Android BSDF2 format support. Compatible with Android OTA payloads.

## Usage

```rust
fn main() {
    let one = vec![1, 2, 3, 4, 5];
    let two = vec![1, 2, 4, 6];
    let mut patch = Vec::new();

    bsdiff_android::diff(&one, &two, &mut patch).unwrap();

    let mut patched = Vec::with_capacity(two.len());
    bsdiff_android::patch(&one, &mut patch.as_slice(), &mut patched).unwrap();
    assert_eq!(patched, two);
}
```

## Diffing Files

```rust
fn diff_files(file_a: &str, file_b: &str, patch_file: &str) -> std::io::Result<()> {
    let old = std::fs::read(file_a)?;
    let new = std::fs::read(file_b)?;
    let mut patch = Vec::new();

    bsdiff_android::diff(&old, &new, &mut patch)?;
    std::fs::write(patch_file, &patch)
}
```

## Patching Files

```rust
fn patch_file(file_a: &str, patch_file: &str, file_b: &str) -> std::io::Result<()> {
    let old = std::fs::read(file_a)?;
    let patch = std::fs::read(patch_file)?;
    let mut new = Vec::new();

    bsdiff_android::patch(&old, &mut patch.as_slice(), &mut new)?;
    std::fs::write(file_b, &new)
}
```

## Android BSDF2 Format

```rust
use bsdiff_android::patch_bsdf2;

fn apply_android_ota(old: &[u8], patch: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut new = Vec::new();
    patch_bsdf2(old, patch, &mut new)?;
    Ok(new)
}
```

## License

BSD-2-Clause

Based on Colin Percival's bsdiff/bspatch algorithm.