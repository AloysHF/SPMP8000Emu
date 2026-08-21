// Data decompression for SPMP8000 BIN files
//
// SPMP8000 NGame1.0 BIN files store ARM code and resources after the
// 0x80-byte header. Based on analysis of actual game files, the data
// is typically UNCOMPRESSED ARM code with embedded resources.
//
// This module provides:
// 1. Raw (uncompressed) data detection and passthrough
// 2. LZ77/LZSS decompression (for files that may use it)
// 3. Zlib decompression (for files that may use it)

use anyhow::Result;

/// Decompression error
#[derive(Debug, thiserror::Error)]
pub enum DecompressError {
    #[error("Unknown compression format")]
    UnknownFormat,
    #[error("Decompression failed: {0}")]
    Failed(String),
    #[error("Buffer too small")]
    BufferTooSmall,
}

const NGAME_DES_KEY: [u8; 8] = [0x6E, 0x2D, 0x70, 0x2A, 0x38, 0x4B, 0x47, 0x4D];
const NGAME_ENCRYPTED_PREFIX_LEN: usize = 0x400;

const IP_TABLE: [usize; 64] = [
    58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6,
    64, 56, 48, 40, 32, 24, 16, 8, 57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61,
    53, 45, 37, 29, 21, 13, 5, 63, 55, 47, 39, 31, 23, 15, 7,
];

const IPR_TABLE: [usize; 64] = [
    40, 8, 48, 16, 56, 24, 64, 32, 39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30,
    37, 5, 45, 13, 53, 21, 61, 29, 36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27,
    34, 2, 42, 10, 50, 18, 58, 26, 33, 1, 41, 9, 49, 17, 57, 25,
];

const E_TABLE: [usize; 48] = [
    32, 1, 2, 3, 4, 5, 4, 5, 6, 7, 8, 9, 8, 9, 10, 11, 12, 13, 12, 13, 14, 15, 16, 17, 16, 17, 18,
    19, 20, 21, 20, 21, 22, 23, 24, 25, 24, 25, 26, 27, 28, 29, 28, 29, 30, 31, 32, 1,
];

const P_TABLE: [usize; 32] = [
    16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10, 2, 8, 24, 14, 32, 27, 3, 9, 19,
    13, 30, 6, 22, 11, 4, 25,
];

const PC1_TABLE: [usize; 56] = [
    57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59, 51, 43, 35, 27, 19, 11, 3, 60,
    52, 44, 36, 63, 55, 47, 39, 31, 23, 15, 7, 62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29,
    21, 13, 5, 28, 20, 12, 4,
];

const PC2_TABLE: [usize; 48] = [
    14, 17, 11, 24, 1, 5, 3, 28, 15, 6, 21, 10, 23, 19, 12, 4, 26, 8, 16, 7, 27, 20, 13, 2, 41, 52,
    31, 37, 47, 55, 30, 40, 51, 45, 33, 48, 44, 49, 39, 56, 34, 53, 46, 42, 50, 36, 29, 32,
];

const LOOP_TABLE: [usize; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];

const S_BOX: [[[u8; 16]; 4]; 8] = [
    [
        [14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7],
        [0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12, 11, 9, 5, 3, 8],
        [4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0],
        [15, 12, 8, 2, 4, 9, 1, 7, 5, 11, 3, 14, 10, 0, 6, 13],
    ],
    [
        [15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10],
        [3, 13, 4, 7, 15, 2, 8, 14, 12, 0, 1, 10, 6, 9, 11, 5],
        [0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15],
        [13, 8, 10, 1, 3, 15, 4, 2, 11, 6, 7, 12, 0, 5, 14, 9],
    ],
    [
        [10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8],
        [13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5, 14, 12, 11, 15, 1],
        [13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7],
        [1, 10, 13, 0, 6, 9, 8, 7, 4, 15, 14, 3, 11, 5, 2, 12],
    ],
    [
        [7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15],
        [13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2, 12, 1, 10, 14, 9],
        [10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4],
        [3, 15, 0, 6, 10, 1, 13, 8, 9, 4, 5, 11, 12, 7, 2, 14],
    ],
    [
        [2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9],
        [14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15, 10, 3, 9, 8, 6],
        [4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14],
        [11, 8, 12, 7, 1, 14, 2, 13, 6, 15, 0, 9, 10, 4, 5, 3],
    ],
    [
        [12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11],
        [10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13, 14, 0, 11, 3, 8],
        [9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6],
        [4, 3, 2, 12, 9, 5, 15, 10, 11, 14, 1, 7, 6, 0, 8, 13],
    ],
    [
        [4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1],
        [13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5, 12, 2, 15, 8, 6],
        [1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2],
        [6, 11, 13, 8, 1, 4, 10, 7, 9, 5, 0, 15, 14, 2, 3, 12],
    ],
    [
        [13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7],
        [1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6, 11, 0, 14, 9, 2],
        [7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8],
        [2, 1, 14, 7, 4, 10, 8, 13, 15, 12, 9, 0, 3, 5, 6, 11],
    ],
];

type SubKeys = [[u8; 48]; 16];

fn decrypt_ngame_payload(data: &[u8]) -> Vec<u8> {
    let mut output = data.to_vec();
    let subkeys = set_subkeys(&NGAME_DES_KEY);
    for block in output[..NGAME_ENCRYPTED_PREFIX_LEN].as_chunks_mut::<8>().0 {
        let mut input = [0u8; 8];
        input.copy_from_slice(block);
        block.copy_from_slice(&des_block(input, &subkeys, false));
    }
    output
}

fn set_subkeys(key: &[u8; 8]) -> SubKeys {
    let mut key_bits = [0u8; 64];
    byte_to_bit(&mut key_bits, key);
    let mut k = [0u8; 56];
    transform(&mut k, &key_bits, &PC1_TABLE);

    let mut subkeys = [[0u8; 48]; 16];
    for (round, loops) in LOOP_TABLE.iter().enumerate() {
        rotate_left_bits(&mut k[..28], *loops);
        rotate_left_bits(&mut k[28..], *loops);
        transform(&mut subkeys[round], &k, &PC2_TABLE);
    }
    subkeys
}

fn des_block(input: [u8; 8], subkeys: &SubKeys, encrypt: bool) -> [u8; 8] {
    let mut bits = [0u8; 64];
    byte_to_bit(&mut bits, &input);
    let mut permuted = [0u8; 64];
    transform(&mut permuted, &bits, &IP_TABLE);
    bits = permuted;

    if encrypt {
        for subkey in subkeys.iter() {
            let old_right: [u8; 32] = bits[32..64].try_into().unwrap();
            let mut f = old_right;
            f_func(&mut f, subkey);
            for i in 0..32 {
                bits[32 + i] = f[i] ^ bits[i];
                bits[i] = old_right[i];
            }
        }
    } else {
        for subkey in subkeys.iter().rev() {
            let old_left: [u8; 32] = bits[..32].try_into().unwrap();
            let mut f = old_left;
            f_func(&mut f, subkey);
            for i in 0..32 {
                bits[i] = f[i] ^ bits[32 + i];
                bits[32 + i] = old_left[i];
            }
        }
    }

    let mut final_bits = [0u8; 64];
    transform(&mut final_bits, &bits, &IPR_TABLE);
    let mut out = [0u8; 8];
    bit_to_byte(&mut out, &final_bits);
    out
}

fn f_func(input: &mut [u8; 32], subkey: &[u8; 48]) {
    let mut expanded = [0u8; 48];
    transform(&mut expanded, input, &E_TABLE);
    for i in 0..48 {
        expanded[i] ^= subkey[i];
    }

    let mut s_out = [0u8; 32];
    for box_idx in 0..8 {
        let chunk = &expanded[box_idx * 6..box_idx * 6 + 6];
        let row = ((chunk[0] << 1) | chunk[5]) as usize;
        let col = ((chunk[1] << 3) | (chunk[2] << 2) | (chunk[3] << 1) | chunk[4]) as usize;
        let val = [S_BOX[box_idx][row][col]];
        byte_to_bit(&mut s_out[box_idx * 4..box_idx * 4 + 4], &val);
    }

    transform(input, &s_out, &P_TABLE);
}

fn transform<const N: usize>(out: &mut [u8; N], input: &[u8], table: &[usize; N]) {
    let mut tmp = [0u8; N];
    for i in 0..N {
        tmp[i] = input[table[i] - 1];
    }
    *out = tmp;
}

fn byte_to_bit(out: &mut [u8], input: &[u8]) {
    for i in 0..out.len() {
        out[i] = (input[i >> 3] >> (i & 7)) & 1;
    }
}

fn bit_to_byte(out: &mut [u8], input: &[u8]) {
    out.fill(0);
    for i in 0..input.len() {
        out[i >> 3] |= input[i] << (i & 7);
    }
}

fn rotate_left_bits(bits: &mut [u8], amount: usize) {
    bits.rotate_left(amount);
}
/// Try to decompress data using various algorithms
pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        return Ok(Vec::new());
    }

    if data.len() >= NGAME_ENCRYPTED_PREFIX_LEN {
        let decrypted = decrypt_ngame_payload(data);
        log::info!(
            "NGame payload DES prefix decrypted ({} bytes)",
            decrypted.len()
        );
        return Ok(decrypted);
    }

    // SPMP8000 NGame1.0 BIN files typically contain uncompressed ARM code
    // after the header. Try raw first as it's the most common case.
    if let Ok(result) = try_raw(data) {
        log::info!("Data is uncompressed ({} bytes)", result.len());
        return Ok(result);
    }

    // Try LZ77/LZSS
    if let Ok(result) = try_lz77(data) {
        log::info!("LZ77 decompression successful ({} bytes)", result.len());
        return Ok(result);
    }

    // Try zlib
    if let Ok(result) = try_zlib(data) {
        log::info!("Zlib decompression successful ({} bytes)", result.len());
        return Ok(result);
    }

    // Try RLE
    if let Ok(result) = try_rle(data) {
        log::info!("RLE decompression successful ({} bytes)", result.len());
        return Ok(result);
    }

    // If all methods fail, return the raw data as-is
    // This is the safest approach for SPMP8000 games which are typically
    // stored as uncompressed ARM code
    log::warn!(
        "Could not determine compression format, returning raw data ({} bytes)",
        data.len()
    );
    Ok(data.to_vec())
}

/// Try raw (uncompressed) data
///
/// For SPMP8000 NGame1.0 BIN files, the data after the header is typically
/// uncompressed ARM code. We use relaxed detection to accept most data
/// as valid ARM code since:
/// 1. ARM instructions have valid condition codes in bits 31-28 (0-14)
/// 2. The data contains valid ARM instruction patterns
/// 3. Resources (images, audio) are embedded alongside code
fn try_raw(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 4 {
        return Err(DecompressError::BufferTooSmall.into());
    }

    // Check if data looks like it could be ARM code or resources
    // ARM instructions are 4-byte aligned and have condition codes in bits 31-28
    let mut valid_count = 0;
    let check_size = data.len().min(1024); // Check first 1KB
    let num_words = check_size / 4;

    for i in 0..num_words {
        let offset = i * 4;
        let word = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);

        // ARM condition code is in bits 31-28
        // Valid codes are 0-14 (15 is NV/unconditional extension)
        let cond = (word >> 28) & 0xF;
        if cond < 15 {
            valid_count += 1;
        }
    }

    // If more than 50% of words have valid ARM condition codes,
    // consider it uncompressed ARM code
    let threshold = num_words / 2;
    if valid_count >= threshold {
        return Ok(data.to_vec());
    }

    Err(DecompressError::UnknownFormat.into())
}

/// Try LZ77/LZSS decompression
///
/// LZ77 format:
/// - Flag byte: each bit indicates if next item is literal (0) or reference (1)
/// - Literal: 1 byte copied directly
/// - Reference: 2 bytes encoding offset and length
fn try_lz77(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 2 {
        return Err(DecompressError::BufferTooSmall.into());
    }

    let mut output = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        let flags = data[pos];
        pos += 1;

        for bit in 0..8 {
            if pos >= data.len() {
                break;
            }

            if (flags & (1 << bit)) != 0 {
                // Compressed reference
                if pos + 1 >= data.len() {
                    return Err(DecompressError::BufferTooSmall.into());
                }

                let b1 = data[pos] as u16;
                let b2 = data[pos + 1] as u16;
                pos += 2;

                let offset = ((b1 << 4) | (b2 >> 4)) as usize;
                let length = ((b2 & 0x0F) + 3) as usize;

                if offset == 0 || offset > output.len() {
                    return Err(DecompressError::Failed("Invalid LZ77 offset".into()).into());
                }

                let start = output.len() - offset;
                for i in 0..length {
                    output.push(output[start + i]);
                }
            } else {
                // Literal byte
                output.push(data[pos]);
                pos += 1;
            }
        }
    }

    if output.is_empty() {
        return Err(DecompressError::Failed("Empty output".into()).into());
    }

    Ok(output)
}

/// Try zlib decompression
fn try_zlib(data: &[u8]) -> Result<Vec<u8>> {
    // Check zlib header
    if data.len() < 2 {
        return Err(DecompressError::BufferTooSmall.into());
    }

    // zlib header: CMF (1 byte) + FLG (1 byte)
    let cmf = data[0] as u16;
    let flg = data[1] as u16;

    // Check valid zlib header
    if !(cmf * 256 + flg).is_multiple_of(31) {
        return Err(DecompressError::Failed("Invalid zlib header".into()).into());
    }

    // For now, just return an error since we don't have a zlib implementation
    // In a real implementation, we'd use a zlib library
    Err(DecompressError::UnknownFormat.into())
}

/// Try RLE (Run-Length Encoding) decompression
fn try_rle(data: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut pos = 0;

    while pos < data.len() {
        let byte = data[pos];
        pos += 1;

        if byte == 0 && pos < data.len() {
            // RLE marker
            let count = data[pos] as usize;
            pos += 1;

            if pos < data.len() {
                let value = data[pos];
                pos += 1;

                for _ in 0..count {
                    output.push(value);
                }
            }
        } else {
            output.push(byte);
        }
    }

    if output.is_empty() {
        return Err(DecompressError::Failed("Empty output".into()).into());
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ngame_des_prefix_decrypts_real_sample() {
        let mut data = vec![0u8; NGAME_ENCRYPTED_PREFIX_LEN];
        data[..16].copy_from_slice(&[
            0x02, 0x3B, 0xA5, 0x73, 0x9A, 0xA1, 0xCA, 0x73, 0xE7, 0x8C, 0x10, 0x7B, 0xEC, 0x95,
            0xB1, 0xAD,
        ]);

        let decrypted = decrypt_ngame_payload(&data);
        assert_eq!(
            &decrypted[..16],
            &[
                0xC0, 0x30, 0x9F, 0xE5, 0xC0, 0x10, 0x9F, 0xE5, 0x0D, 0xC0, 0xA0, 0xE1, 0x01, 0x00,
                0x53, 0xE1,
            ]
        );
    }
    #[test]
    fn test_raw_detection_arm_code() {
        // STMFD SP!, {R4-R7, LR} followed by other ARM instructions
        let data = vec![
            0xF0, 0x47, 0x2D, 0xE9, // STMFD SP!, {R4-R7, LR}
            0x04, 0xE0, 0x2D, 0xE5, // STR LR, [SP, #-4]!
            0x00, 0x40, 0xA0, 0xE1, // MOV R4, R0
        ];
        assert!(try_raw(&data).is_ok());
    }

    #[test]
    fn test_raw_detection_mixed_data() {
        // Mixed data with valid ARM condition codes
        // Most ARM condition codes are valid (0-14), so random data
        // with valid condition codes should pass
        let data = vec![
            0x02, 0x3B, 0xA5, 0x73, // Valid ARM (cond=7)
            0x9A, 0xA1, 0xCA, 0x73, // Valid ARM (cond=7)
            0xE7, 0x8C, 0x10, 0x7B, // Valid ARM (cond=7)
        ];
        assert!(try_raw(&data).is_ok());
    }

    #[test]
    fn test_raw_detection_invalid() {
        // Data with all invalid condition codes (15 = NV)
        let data = vec![
            0xFF, 0xFF, 0xFF, 0xFF, // NV condition code
            0xFF, 0xFF, 0xFF, 0xFF,
        ];
        assert!(try_raw(&data).is_err());
    }

    #[test]
    fn test_lz77_basic() {
        // Simple LZ77 test case
        let _data = [
            0x01, // flags: bit 0 set
            0x00, 0x33, // reference: offset=0, length=3+3=6
            0x41, 0x42, 0x43, // literal "ABC"
        ];
        // This would decompress to something, but the test data is not valid
        // In a real test, we'd use actual compressed data
    }

    #[test]
    fn test_decompress_returns_raw_for_arm_data() {
        // Test that decompress returns raw data for typical ARM code
        let data = vec![
            0xF0, 0x47, 0x2D, 0xE9, // STMFD SP!, {R4-R7, LR}
            0x04, 0xE0, 0x2D, 0xE5, // STR LR, [SP, #-4]!
        ];
        let result = decompress(&data).unwrap();
        assert_eq!(result, data);
    }
}
