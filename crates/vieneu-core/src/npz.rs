//! Minimal `.npz` reader for the tied embedding heads.
//!
//! `vieneu_v3_heads.npz` is a ZIP of two `.npy` arrays stored as **float16**:
//!   - `text_emb`  : (text_vocab, hidden)
//!   - `audio_emb` : (n_vq, audio_vocab, hidden)
//!
//! `ndarray-npy` has no first-class f16 support, so we parse the `.npy` headers
//! ourselves and widen f16 → f32 on load.

use anyhow::{anyhow, bail, Context, Result};
use half::f16;
use ndarray::{Array2, Array3};
use std::io::Read;
use std::path::Path;

/// The two embedding tables, widened to f32 and ready for matmuls.
pub struct Heads {
    /// (text_vocab, hidden)
    pub text_emb: Array2<f32>,
    /// (n_vq, audio_vocab, hidden)
    pub audio_emb: Array3<f32>,
}

pub fn load_heads(npz_path: &Path) -> Result<Heads> {
    let file = std::fs::File::open(npz_path)
        .with_context(|| format!("open npz {}", npz_path.display()))?;
    let mut zip = zip::ZipArchive::new(file).context("read npz as zip")?;

    let text = read_npy_f32(&mut zip, "text_emb.npy")?;
    let audio = read_npy_f32(&mut zip, "audio_emb.npy")?;

    let text_emb = to_array2(text)?;
    let audio_emb = to_array3(audio)?;
    Ok(Heads {
        text_emb,
        audio_emb,
    })
}

/// Raw decoded array: row-major f32 data plus its shape.
struct RawArray {
    shape: Vec<usize>,
    data: Vec<f32>,
}

fn read_npy_f32<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<RawArray> {
    let mut entry = zip
        .by_name(name)
        .with_context(|| format!("npz entry {name} missing"))?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut bytes)?;
    parse_npy(&bytes).with_context(|| format!("parse {name}"))
}

/// Parse a `.npy` v1.0/v2.0 buffer of dtype float16 or float32 (little-endian,
/// C order) into f32.
fn parse_npy(buf: &[u8]) -> Result<RawArray> {
    if buf.len() < 10 || &buf[0..6] != b"\x93NUMPY" {
        bail!("not a .npy file");
    }
    let major = buf[6];
    // header length field width differs between v1 (u16) and v2 (u32).
    let (header_len, header_start) = if major >= 2 {
        (u32::from_le_bytes(buf[8..12].try_into()?) as usize, 12usize)
    } else {
        (u16::from_le_bytes(buf[8..10].try_into()?) as usize, 10usize)
    };
    let header = std::str::from_utf8(&buf[header_start..header_start + header_len])
        .context("npy header utf8")?;
    let data_start = header_start + header_len;

    let descr = extract_field(header, "descr")?;
    let fortran = header.contains("'fortran_order': True");
    if fortran {
        bail!("fortran-order npy not supported (expected C order)");
    }
    let shape = parse_shape(header)?;
    let count: usize = shape.iter().product();

    let body = &buf[data_start..];
    let data = match descr.as_str() {
        "<f2" | "|f2" | "float16" => {
            if body.len() < count * 2 {
                bail!("f16 body too short");
            }
            (0..count)
                .map(|i| {
                    let b = [body[i * 2], body[i * 2 + 1]];
                    f16::from_le_bytes(b).to_f32()
                })
                .collect()
        }
        "<f4" | "|f4" | "float32" => {
            if body.len() < count * 4 {
                bail!("f32 body too short");
            }
            (0..count)
                .map(|i| f32::from_le_bytes(body[i * 4..i * 4 + 4].try_into().unwrap()))
                .collect()
        }
        other => bail!("unsupported npy dtype {other}"),
    };
    Ok(RawArray { shape, data })
}

fn extract_field(header: &str, key: &str) -> Result<String> {
    let pat = format!("'{key}':");
    let idx = header
        .find(&pat)
        .ok_or_else(|| anyhow!("npy header missing {key}"))?;
    let rest = &header[idx + pat.len()..];
    let start = rest
        .find('\'')
        .ok_or_else(|| anyhow!("npy header {key} value"))?;
    let after = &rest[start + 1..];
    let end = after
        .find('\'')
        .ok_or_else(|| anyhow!("npy header {key} end"))?;
    Ok(after[..end].to_string())
}

fn parse_shape(header: &str) -> Result<Vec<usize>> {
    let idx = header
        .find("'shape':")
        .ok_or_else(|| anyhow!("npy header missing shape"))?;
    let rest = &header[idx..];
    let open = rest.find('(').ok_or_else(|| anyhow!("shape open paren"))?;
    let close = rest.find(')').ok_or_else(|| anyhow!("shape close paren"))?;
    let inner = &rest[open + 1..close];
    let dims = inner
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<usize>()
                .map_err(|e| anyhow!("shape dim {s}: {e}"))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(dims)
}

fn to_array2(raw: RawArray) -> Result<Array2<f32>> {
    if raw.shape.len() != 2 {
        bail!("expected 2D array, got {:?}", raw.shape);
    }
    Array2::from_shape_vec((raw.shape[0], raw.shape[1]), raw.data).context("reshape 2D")
}

fn to_array3(raw: RawArray) -> Result<Array3<f32>> {
    if raw.shape.len() != 3 {
        bail!("expected 3D array, got {:?}", raw.shape);
    }
    Array3::from_shape_vec((raw.shape[0], raw.shape[1], raw.shape[2]), raw.data)
        .context("reshape 3D")
}
