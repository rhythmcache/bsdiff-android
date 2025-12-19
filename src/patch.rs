/*-
 * Copyright 2003-2005 Colin Percival
 * Copyright 2012 Matthew Endsley
 * Modified 2017 Pieter-Jan Briers
 * Modified 2021 Kornel Lesinski
 * Modified 2025 - Performance optimizations and validation
 * All rights reserved
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted providing that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS OR
 * IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
 * WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
 * DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT,
 * STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING
 * IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */

use std::io;
use std::io::Read;
use std::ops::DerefMut;

/// Apply a patch to an "old" file, returning the "new" file.
///
/// `old` is the old file, `patch` will be read from with the patch, `new` is the buffer that will be written into.
///
/// This is optimized for performance with:
/// - Bulk read operations
/// - SIMD-friendly memory access patterns
/// - Proper validation with early errors
pub fn patch<T, W>(old: &[u8], patch: &mut T, new: &mut W) -> io::Result<()>
where
    T: Read,
    W: io::Write + DerefMut<Target = [u8]>,
{
    let mut oldpos: usize = 0;
    
    loop {
        // Read control data
        let mut buf = [0; 24];
        if read_or_eof(patch, &mut buf)? {
            return Ok(());
        }

        // Decode using bspatch sign-magnitude encoding (NOT plain LE)
        // This matches AOSP/bspatch behavior
        let mix_len_raw = offtin(buf[0..8].try_into().unwrap());
        let copy_len_raw = offtin(buf[8..16].try_into().unwrap());
        let seek_len = offtin(buf[16..24].try_into().unwrap());

        // Validate lengths are non-negative
        if mix_len_raw < 0 || copy_len_raw < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Negative length: mix={}, copy={}", mix_len_raw, copy_len_raw),
            ));
        }

        let mix_len = mix_len_raw as usize;
        let copy_len = copy_len_raw as usize;

        // Check for overflow before reading
        let to_read = mix_len
            .checked_add(copy_len)
            .ok_or(io::ErrorKind::InvalidData)?;

        // Read diff string and literal data at once (bulk operation)
        let mix_start = new.len();
        let mut read_from = patch.take(to_read as u64);
        let has_read = io::copy(&mut read_from, new)?;

        if has_read != to_read as u64 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        // Compute mix range with overflow checks
        let mix_end = mix_start
            .checked_add(mix_len)
            .ok_or(io::ErrorKind::InvalidData)?;

        let mix_slice = new
            .get_mut(mix_start..mix_end)
            .ok_or(io::ErrorKind::UnexpectedEof)?;

        // Compute old range with overflow checks
        let oldpos_end = oldpos
            .checked_add(mix_len)
            .ok_or(io::ErrorKind::InvalidData)?;

        let old_slice = old
            .get(oldpos..oldpos_end)
            .ok_or(io::ErrorKind::UnexpectedEof)?;

        // Mix operation: new[i] += old[i]
        // This is optimized for SIMD and cache locality
        for (n, o) in mix_slice.iter_mut().zip(old_slice.iter().copied()) {
            *n = n.wrapping_add(o);
        }

        // Adjust oldpos with mix_len
        oldpos += mix_len;

        // Apply seek with proper validation
        // CRITICAL FIX: Check for underflow before converting
        let new_oldpos = (oldpos as i64)
            .checked_add(seek_len)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Seek overflow: oldpos={}, seek={}", oldpos, seek_len),
                )
            })?;

        if new_oldpos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Seek underflow: oldpos={}, seek={}", oldpos, seek_len),
            ));
        }

        oldpos = new_oldpos as usize;
    }
}

/// It allows EOF only before the first byte.
/// Optimized to minimize syscalls
#[inline]
fn read_or_eof<T: Read>(reader: &mut T, buf: &mut [u8; 24]) -> io::Result<bool> {
    let mut tmp = &mut buf[..];
    loop {
        match reader.read(tmp) {
            Ok(0) => {
                return if tmp.len() == 24 {
                    Ok(true) // Clean EOF at start
                } else {
                    Err(io::ErrorKind::UnexpectedEof.into())
                }
            }
            Ok(n) => {
                if n >= tmp.len() {
                    return Ok(false);
                }
                tmp = &mut tmp[n..];
            }
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// Reads sign-magnitude i64 as used in bspatch
/// This is the CORRECT encoding used by AOSP and classic bspatch
/// NOT plain little-endian i64!
#[inline]
fn offtin(buf: [u8; 8]) -> i64 {
    let y = i64::from_le_bytes(buf);
    if 0 == y & (1 << 63) {
        y
    } else {
        -(y & !(1 << 63))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offtin_zero() {
        let buf = [0u8; 8];
        assert_eq!(offtin(buf), 0);
    }

    #[test]
    fn test_offtin_positive() {
        let buf = [42, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(offtin(buf), 42);
    }

    #[test]
    fn test_offtin_negative() {
        // 42 with sign bit set
        let buf = [42, 0, 0, 0, 0, 0, 0, 0x80];
        assert_eq!(offtin(buf), -42);
    }

    #[test]
    fn test_offtin_max_positive() {
        // Maximum positive value: 0x7FFFFFFFFFFFFFFF
        let buf = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x7F];
        assert_eq!(offtin(buf), i64::MAX);
    }

    #[test]
    fn test_offtin_max_negative() {
        // Maximum negative value: sign bit + 0x7FFFFFFFFFFFFFFF
        let buf = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(offtin(buf), -i64::MAX);
    }
}