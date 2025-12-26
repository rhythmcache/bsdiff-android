use std::io::{self, Write};
use bzip2::write::BzEncoder;
use bzip2::Compression as BzCompression;

const BSDIFF_MAGIC: &[u8; 8] = b"BSDIFF40";
const BSDF2_MAGIC: &[u8; 5] = b"BSDF2";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionAlgorithm {
    None = 0,
    Bz2 = 1,
    Brotli = 2,
}

fn compress(alg: CompressionAlgorithm, data: &[u8]) -> io::Result<Vec<u8>> {
    match alg {
        CompressionAlgorithm::None => Ok(data.to_vec()),
        CompressionAlgorithm::Bz2 => {
            let mut encoder = BzEncoder::new(Vec::new(), BzCompression::best());
            encoder.write_all(data)?;
            encoder.finish()
        }
        CompressionAlgorithm::Brotli => {
            let mut compressed = Vec::new();
            {
                let mut encoder = brotli::CompressorWriter::new(
                    &mut compressed,
                    4096,  // buffer size
                    11,    // quality (11 = max)
                    20,    // lg_window_size (matches Android kBrotliDefaultLgwin)
                );
                encoder.write_all(data)?;
                encoder.flush()?;
            } // encoder drops here, finalizing compression
            Ok(compressed)
        }
    }
}

/// encode signed integer in bspatch sign-magnitude format
#[inline]
fn encode_int64(x: i64, buf: &mut [u8]) {
    if x >= 0 {
        buf.copy_from_slice(&x.to_le_bytes());
    } else {
        let tmp = ((-x) as u64) | (1u64 << 63);
        buf.copy_from_slice(&tmp.to_le_bytes());
    }
}

/// Control entry matching
#[derive(Debug, Clone, Copy)]
pub struct ControlEntry {
    pub diff_size: i64,
    pub extra_size: i64,
    pub offset_increment: i64,
}

/// BSDF2 patch writer
pub struct Bsdf2Writer {
    ctrl_data: Vec<u8>,
    diff_data: Vec<u8>,
    extra_data: Vec<u8>,
    ctrl_alg: CompressionAlgorithm,
    diff_alg: CompressionAlgorithm,
    extra_alg: CompressionAlgorithm,
    written_output: u64,
}

impl Bsdf2Writer {
    /// Create a new BSDF2 writer with specified compression for each stream
    pub fn new(
        ctrl_alg: CompressionAlgorithm,
        diff_alg: CompressionAlgorithm,
        extra_alg: CompressionAlgorithm,
    ) -> Self {
        Self {
            ctrl_data: Vec::new(),
            diff_data: Vec::new(),
            extra_data: Vec::new(),
            ctrl_alg,
            diff_alg,
            extra_alg,
            written_output: 0,
        }
    }
    pub fn new_legacy() -> Self {
        Self::new(
            CompressionAlgorithm::Bz2,
            CompressionAlgorithm::Bz2,
            CompressionAlgorithm::Bz2,
        )
    }
    pub fn add_control_entry(&mut self, entry: ControlEntry) -> io::Result<()> {
        let mut buf = [0u8; 24];
        encode_int64(entry.diff_size, &mut buf[0..8]);
        encode_int64(entry.extra_size, &mut buf[8..16]);
        encode_int64(entry.offset_increment, &mut buf[16..24]);

        self.ctrl_data.extend_from_slice(&buf);
        self.written_output += (entry.diff_size + entry.extra_size) as u64;
        Ok(())
    }

    /// Write diff stream data
    pub fn write_diff_stream(&mut self, data: &[u8]) -> io::Result<()> {
        self.diff_data.extend_from_slice(data);
        Ok(())
    }
    pub fn write_extra_stream(&mut self, data: &[u8]) -> io::Result<()> {
        self.extra_data.extend_from_slice(data);
        Ok(())
    }
    pub fn close<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        // Compress all streams
        let ctrl_compressed = compress(self.ctrl_alg, &self.ctrl_data)?;
        let diff_compressed = compress(self.diff_alg, &self.diff_data)?;
        let extra_compressed = compress(self.extra_alg, &self.extra_data)?;

        // Write header
        let is_legacy = self.ctrl_alg == CompressionAlgorithm::Bz2
            && self.diff_alg == CompressionAlgorithm::Bz2
            && self.extra_alg == CompressionAlgorithm::Bz2;

        self.write_header(
            writer,
            is_legacy,
            ctrl_compressed.len() as u64,
            diff_compressed.len() as u64,
        )?;

        // Write compressed streams
        writer.write_all(&ctrl_compressed)?;
        writer.write_all(&diff_compressed)?;
        writer.write_all(&extra_compressed)?;

        Ok(())
    }

    fn write_header<W: Write>(
        &self,
        writer: &mut W,
        is_legacy: bool,
        ctrl_size: u64,
        diff_size: u64,
    ) -> io::Result<()> {
        let mut header = [0u8; 32];

        if is_legacy {
            header[0..8].copy_from_slice(BSDIFF_MAGIC);
        } else {
            header[0..5].copy_from_slice(BSDF2_MAGIC);
            header[5] = self.ctrl_alg as u8;
            header[6] = self.diff_alg as u8;
            header[7] = self.extra_alg as u8;
        }

        encode_int64(ctrl_size as i64, &mut header[8..16]);
        encode_int64(diff_size as i64, &mut header[16..24]);
        encode_int64(self.written_output as i64, &mut header[24..32]);

        writer.write_all(&header)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_int64_positive() {
        let mut buf = [0u8; 8];
        encode_int64(42, &mut buf);
        assert_eq!(buf, [42, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn test_encode_int64_negative() {
        let mut buf = [0u8; 8];
        encode_int64(-42, &mut buf);
        assert_eq!(buf, [42, 0, 0, 0, 0, 0, 0, 0x80]);
    }

    #[test]
    fn test_encode_int64_zero() {
        let mut buf = [0u8; 8];
        encode_int64(0, &mut buf);
        assert_eq!(buf, [0; 8]);
    }

    #[test]
    fn test_writer_creation() {
        let writer = Bsdf2Writer::new(
            CompressionAlgorithm::Brotli,
            CompressionAlgorithm::Brotli,
            CompressionAlgorithm::Bz2,
        );
        assert_eq!(writer.ctrl_alg, CompressionAlgorithm::Brotli);
        assert_eq!(writer.written_output, 0);
    }

    #[test]
    fn test_legacy_writer() {
        let writer = Bsdf2Writer::new_legacy();
        assert_eq!(writer.ctrl_alg, CompressionAlgorithm::Bz2);
        assert_eq!(writer.diff_alg, CompressionAlgorithm::Bz2);
        assert_eq!(writer.extra_alg, CompressionAlgorithm::Bz2);
    }
}
