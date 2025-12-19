// bsdf2.rs - Android BSDF2 format

use std::io::{self, Read};

const BSDIFF_MAGIC: &[u8; 8] = b"BSDIFF40";
const BSDF2_MAGIC: &[u8; 5] = b"BSDF2";

// Safety limits to prevent OOM
// const MAX_PATCH_SIZE: usize = 512 * 1024 * 1024; // 512 MB
const MAX_NEW_SIZE: usize = 2 * 1024 * 1024 * 1024; // 2 GB

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    None = 0,
    Bz2 = 1,
    Brotli = 2,
}

impl CompressionAlgorithm {
    fn from_u8(value: u8) -> io::Result<Self> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Bz2),
            2 => Ok(Self::Brotli),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown compression algorithm: {}", value),
            )),
        }
    }
}

/// Reads sign-magnitude i64 as used in bspatch
/// This is NOT plain little-endian - it uses sign-magnitude encoding
#[inline]
fn offtin(buf: [u8; 8]) -> i64 {
    let y = i64::from_le_bytes(buf);
    if 0 == y & (1 << 63) {
        y
    } else {
        -(y & !(1 << 63))
    }
}

/// Decompress data based on algorithm
fn decompress(alg: CompressionAlgorithm, data: &[u8]) -> io::Result<Vec<u8>> {
    match alg {
        CompressionAlgorithm::None => Ok(data.to_vec()),
        CompressionAlgorithm::Bz2 => {
            let mut decompressed = Vec::new();
            let mut decoder = bzip2::read::BzDecoder::new(data);
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        }
        CompressionAlgorithm::Brotli => {
            let mut decompressed = Vec::new();
            let mut decoder = brotli::Decompressor::new(data, 4096);
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        }
    }
}

/// Parse BSDF2 or classic BSDIFF patch header and return streams
pub fn parse_bsdf2_header(
    patch_data: &[u8],
) -> io::Result<(i64, Vec<u8>, Vec<u8>, Vec<u8>)> {
    if patch_data.len() < 32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Patch data too short",
        ));
    }

    let magic = &patch_data[0..8];

    // Determine format and compression algorithms
    let (alg_control, alg_diff, alg_extra) = if magic == BSDIFF_MAGIC {
        // Classic BSDIFF format - uses BZ2 for all streams
        (
            CompressionAlgorithm::Bz2,
            CompressionAlgorithm::Bz2,
            CompressionAlgorithm::Bz2,
        )
    } else if &magic[0..5] == BSDF2_MAGIC {
        // BSDF2 format - per-stream compression
        (
            CompressionAlgorithm::from_u8(magic[5])?,
            CompressionAlgorithm::from_u8(magic[6])?,
            CompressionAlgorithm::from_u8(magic[7])?,
        )
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid BSDIFF/BSDF2 magic header",
        ));
    };

    // Read length headers using bspatch integer encoding
    let len_control = offtin(patch_data[8..16].try_into().unwrap());
    let len_diff = offtin(patch_data[16..24].try_into().unwrap());
    let new_size = offtin(patch_data[24..32].try_into().unwrap());

    // Validate lengths before allocation
    if len_control < 0 || len_diff < 0 || new_size < 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Negative length in patch header",
        ));
    }

    let len_control = len_control as usize;
    let len_diff = len_diff as usize;
    let new_size_usize = new_size as usize;

    // Safety checks before allocation
    if new_size_usize > MAX_NEW_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("New size {} exceeds limit", new_size_usize),
        ));
    }

    
    
    let pos: usize = 32;


    // Validate lengths don't exceed patch bounds
    if pos.checked_add(len_control)
        .and_then(|p| p.checked_add(len_diff))
        .map_or(true, |total| total > patch_data.len())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Stream lengths exceed patch size",
        ));
    }

    // Read and decompress control stream
    let control_end = pos + len_control;
    if control_end > patch_data.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Control stream exceeds patch bounds",
        ));
    }
    let control_compressed = &patch_data[pos..control_end];
    let control_data = decompress(alg_control, control_compressed)?;

    // Validate control data is properly aligned (24 bytes per tuple)
    if control_data.len() % 24 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid control data length (not multiple of 24)",
        ));
    }

    // Read and decompress diff stream
    let diff_start = control_end;
    let diff_end = diff_start + len_diff;
    if diff_end > patch_data.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Diff stream exceeds patch bounds",
        ));
    }
    let diff_compressed = &patch_data[diff_start..diff_end];
    let diff_data = decompress(alg_diff, diff_compressed)?;

    // Read and decompress extra stream (rest of data)
    let extra_compressed = &patch_data[diff_end..];
    let extra_data = decompress(alg_extra, extra_compressed)?;

    Ok((new_size, control_data, diff_data, extra_data))
}

/// Apply a BSDF2 patch with full validation and optimizations
pub fn patch_bsdf2(old: &[u8], patch_data: &[u8], new: &mut Vec<u8>) -> io::Result<()> {
    // Parse header and decompress streams
    let (new_size, control_data, diff_data, extra_data) = parse_bsdf2_header(patch_data)?;

    let new_size = new_size as usize;

    // Pre-allocate output buffer
    new.clear();
    new.reserve(new_size);

    let mut oldpos: usize = 0;
    let mut diff_pos: usize = 0;
    let mut extra_pos: usize = 0;

    // Process control tuples
    let mut ctrl_idx = 0;
    while ctrl_idx < control_data.len() {
        if ctrl_idx + 24 > control_data.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Incomplete control tuple",
            ));
        }

        // Read control tuple using bspatch integer encoding
        let add_len = offtin(control_data[ctrl_idx..ctrl_idx + 8].try_into().unwrap());
        let copy_len = offtin(control_data[ctrl_idx + 8..ctrl_idx + 16].try_into().unwrap());
        let seek_amount = offtin(control_data[ctrl_idx + 16..ctrl_idx + 24].try_into().unwrap());
        ctrl_idx += 24;

        // Validate lengths
        if add_len < 0 || copy_len < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Negative length in control tuple: add={}, copy={}", add_len, copy_len),
            ));
        }

        let add_len = add_len as usize;
        let copy_len = copy_len as usize;

        // Check we won't exceed output size
        if new.len().checked_add(add_len)
            .and_then(|n| n.checked_add(copy_len))
            .map_or(true, |total| total > new_size)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Control tuple would exceed new_size",
            ));
        }

        // ADD operation: new[newpos..newpos+add] = old[oldpos..] + diff[diff_pos..]
        if add_len > 0 {
            // Check diff_data bounds
            if diff_pos.checked_add(add_len).map_or(true, |end| end > diff_data.len()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Diff data exhausted",
                ));
            }

            // Optimized: reserve space and write directly
            let new_start = new.len();
            new.resize(new_start + add_len, 0);

            // SIMD-friendly loop: compute in chunks
            for i in 0..add_len {
                let old_byte = old.get(oldpos + i).copied().unwrap_or(0);
                let diff_byte = diff_data[diff_pos + i];
                new[new_start + i] = old_byte.wrapping_add(diff_byte);
            }

            oldpos = oldpos.saturating_add(add_len);
            diff_pos += add_len;
        }

        // COPY operation: new[newpos..newpos+copy] = extra[extra_pos..]
        if copy_len > 0 {
            // Check extra_data bounds
            if extra_pos.checked_add(copy_len).map_or(true, |end| end > extra_data.len()) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Extra data exhausted",
                ));
            }

            new.extend_from_slice(&extra_data[extra_pos..extra_pos + copy_len]);
            extra_pos += copy_len;
        }

        // SEEK operation: adjust oldpos
        // CRITICAL: Validate seek doesn't underflow
        let new_oldpos = (oldpos as i64)
            .checked_add(seek_amount)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Seek overflow")
            })?;

        if new_oldpos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Seek underflow: oldpos={}, seek={}", oldpos, seek_amount),
            ));
        }

        oldpos = new_oldpos as usize;
    }

    // Validate final state
    if new.len() != new_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Final size mismatch: expected {}, got {}", new_size, new.len()),
        ));
    }

    // Validate all streams were fully consumed
    if diff_pos != diff_data.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Diff data not fully consumed: used {}/{}", diff_pos, diff_data.len()),
        ));
    }

    if extra_pos != extra_data.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Extra data not fully consumed: used {}/{}", extra_pos, extra_data.len()),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offtin_positive() {
        let buf = [0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert_eq!(offtin(buf), 0x42);
    }

    #[test]
    fn test_offtin_negative() {
        // Sign bit set: 0x8000000000000042 represents -66
        let buf = [0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80];
        assert_eq!(offtin(buf), -0x42);
    }

    #[test]
    fn test_compression_algorithm_from_u8() {
        assert_eq!(CompressionAlgorithm::from_u8(0).unwrap(), CompressionAlgorithm::None);
        assert_eq!(CompressionAlgorithm::from_u8(1).unwrap(), CompressionAlgorithm::Bz2);
        assert_eq!(CompressionAlgorithm::from_u8(2).unwrap(), CompressionAlgorithm::Brotli);
        assert!(CompressionAlgorithm::from_u8(3).is_err());
    }

    #[test]
    fn test_parse_invalid_magic() {
        let invalid = vec![0u8; 32];
        assert!(parse_bsdf2_header(&invalid).is_err());
    }

    #[test]
    fn test_parse_negative_lengths() {
        let mut data = vec![0u8; 32];
        data[0..8].copy_from_slice(BSDIFF_MAGIC);
        // Set negative length (sign bit set)
        data[8] = 0x01;
        data[15] = 0x80; // Sign bit
        
        assert!(parse_bsdf2_header(&data).is_err());
    }
}