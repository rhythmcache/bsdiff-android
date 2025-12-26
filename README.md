# bsdiff-android

[![GitHub](https://img.shields.io/badge/github-bsdiff--android-8da0cb?logo=github)](https://github.com/YOUR_USERNAME/bsdiff-android)
[![crates.io version](https://img.shields.io/crates/v/bsdiff-android.svg)](https://crates.io/crates/bsdiff-android)
[![docs.rs docs](https://docs.rs/bsdiff-android/badge.svg)](https://docs.rs/bsdiff-android)
[![CI build](https://github.com/YOUR_USERNAME/bsdiff-android/actions/workflows/rust.yml/badge.svg)](https://github.com/YOUR_USERNAME/bsdiff-android/actions)

Bsdiff/bspatch implementation with Android BSDF2 format support. Compatible with Android OTA payloads.

## Features

-  Classic bsdiff/bspatch (raw format)
-  BSDIFF40 format (BZ2 compressed, compatible with original tools)
-  Android BSDF2 format (Brotli/BZ2/None compression)
-  Fast suffix array construction


## Usage Examples

```rust,ignore
use bsdiff_android as bsdiff;

fn main() {
    let old = vec![1, 2, 3, 4, 5];
    let new = vec![1, 2, 4, 6];
    let mut patch = Vec::new();

    // Generate and apply patch
    bsdiff::diff(&old, &new, &mut patch).unwrap();
    
    let mut patched = Vec::new();
    bsdiff::patch(&old, &mut patch.as_slice(), &mut patched).unwrap();
    assert_eq!(patched, new);
}
```


### Classic BSDIFF40 Format

```rust,ignore
use bsdiff_android::{diff_bsdiff40, patch};

// Generate BSDIFF40 patch (compatible with original bsdiff tools)
let mut patch = Vec::new();
diff_bsdiff40(&old, &new, &mut patch)?;

// Apply patch
let mut result = Vec::new();
patch(&old, &mut patch.as_slice(), &mut result)?;
```

### Android BSDF2 Format (OTA Updates)

```rust,ignore
use bsdiff_android::{diff_bsdf2_uniform, patch_bsdf2, CompressionAlgorithm};

// Generate BSDF2 patch with Brotli compression (Android standard)
let mut patch = Vec::new();
diff_bsdf2_uniform(&old, &new, &mut patch, CompressionAlgorithm::Brotli)?;

// Apply BSDF2 patch
let mut result = Vec::new();
patch_bsdf2(&old, &patch, &mut result)?;
```

### File Operations

```rust,ignore
use bsdiff_android::{diff_bsdf2_uniform, patch_bsdf2, CompressionAlgorithm};
use std::fs;

fn create_update_package() -> std::io::Result<()> {
    // Read old and new versions
    let old = fs::read("app-v1.apk")?;
    let new = fs::read("app-v2.apk")?;
    
    // Generate Android OTA patch
    let mut patch = Vec::new();
    diff_bsdf2_uniform(&old, &new, &mut patch, CompressionAlgorithm::Brotli)?;
    fs::write("update.bsdf2", &patch)?;
    
    println!("Patch size: {} bytes", patch.len());
    Ok(())
}

fn apply_update() -> std::io::Result<()> {
    let old = fs::read("app-v1.apk")?;
    let patch = fs::read("update.bsdf2")?;
    
    let mut new = Vec::new();
    patch_bsdf2(&old, &patch, &mut new)?;
    fs::write("app-v2.apk", &new)?;
    
    Ok(())
}
```

### Mixed Compression (Advanced)

```rust,ignore
use bsdiff_android::{diff_bsdf2, CompressionAlgorithm};

// Use different compression for each stream
let mut patch = Vec::new();
diff_bsdf2(
    &old,
    &new,
    &mut patch,
    CompressionAlgorithm::Brotli,  // Control stream
    CompressionAlgorithm::Brotli,  // Diff stream  
    CompressionAlgorithm::Bz2,     // Extra stream
)?;
```

## API Summary

| Use Case | Generation | Application |
|----------|-----------|-------------|
| Raw format | `diff()` | `patch()` |
| Classic BSDIFF40 | `diff_bsdiff40()` | `patch()` |
| Android BSDF2 | `diff_bsdf2_uniform()` | `patch_bsdf2()` |

## Compression Types

```rust,ignore
CompressionAlgorithm::None    // No compression
CompressionAlgorithm::Bz2     // BZ2 compression
CompressionAlgorithm::Brotli  // Brotli (recommended for Android)
```

## License

BSD-2-Clause

Based on Colin Percival's bsdiff/bspatch algorithm with Android BSDF2 extensions.
