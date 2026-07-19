// Native reader for the Clone Hero .sng container (SNGPKG v1): header with a
// 16-byte XOR mask, ini-style metadata pairs, a file index, then masked file
// data. A Chorus Encore download dropped into songs/ plays with no unpacking.
//
// Only the header, metadata, and file index are parsed at open time; contained
// files are read on demand by seeking into the data section. Scanning a large
// library therefore never pulls whole song archives (audio included) into
// memory just to list titles.
//
// The browser demo has no filesystem, so there the container is opened from
// bytes fetched over HTTP (`from_bytes`) and reads slice into that buffer;
// the header format and unmasking are shared with the native path.

use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::fs::File;
use std::io::Read;
#[cfg(not(target_arch = "wasm32"))]
use std::io::{BufReader, Seek, SeekFrom};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

pub struct SngFile {
    pub metadata: Vec<(String, String)>,
    files: HashMap<String, (u64, u64)>, // name -> (offset, len)
    #[cfg(not(target_arch = "wasm32"))]
    path: PathBuf,
    #[cfg(target_arch = "wasm32")]
    data: std::sync::Arc<Vec<u8>>,
    xor_mask: [u8; 16],
}

fn fill(r: &mut impl Read, buf: &mut [u8]) -> Result<(), String> {
    r.read_exact(buf).map_err(|_| "truncated header".to_string())
}

fn u8_of(r: &mut impl Read) -> Result<u8, String> {
    let mut b = [0u8; 1];
    fill(r, &mut b)?;
    Ok(b[0])
}

fn i32_of(r: &mut impl Read) -> Result<i32, String> {
    let mut b = [0u8; 4];
    fill(r, &mut b)?;
    Ok(i32::from_le_bytes(b))
}

fn u64_of(r: &mut impl Read) -> Result<u64, String> {
    let mut b = [0u8; 8];
    fill(r, &mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn string_of(r: &mut impl Read, n: usize) -> Result<String, String> {
    if n > (1 << 20) {
        return Err("corrupt header (oversized string)".into());
    }
    let mut b = vec![0u8; n];
    fill(r, &mut b)?;
    Ok(String::from_utf8_lossy(&b).to_string())
}

type Header = (Vec<(String, String)>, HashMap<String, (u64, u64)>, [u8; 16]);

/// Parse the SNGPKG header, metadata pairs, and file index from any reader.
fn parse_header(r: &mut impl Read) -> Result<Header, String> {
    let mut magic = [0u8; 6];
    fill(r, &mut magic)?;
    if &magic != b"SNGPKG" {
        return Err("not an SNGPKG container".into());
    }
    let mut version = [0u8; 4];
    fill(r, &mut version)?;
    let mut xor_mask = [0u8; 16];
    fill(r, &mut xor_mask)?;

    let _meta_len = u64_of(r)?;
    let meta_count = u64_of(r)?;
    let mut metadata = Vec::new();
    for _ in 0..meta_count {
        let klen = i32_of(r)?.max(0) as usize;
        let key = string_of(r, klen)?;
        let vlen = i32_of(r)?.max(0) as usize;
        let val = string_of(r, vlen)?;
        metadata.push((key, val));
    }

    let _index_len = u64_of(r)?;
    let file_count = u64_of(r)?;
    let mut files = HashMap::new();
    for _ in 0..file_count {
        let nlen = u8_of(r)? as usize;
        let name = string_of(r, nlen)?;
        let len = u64_of(r)?;
        let offset = u64_of(r)?;
        files.insert(name.to_lowercase(), (offset, len));
    }
    Ok((metadata, files, xor_mask))
}

fn unmask(raw: &mut [u8], mask: &[u8; 16]) {
    for (i, b) in raw.iter_mut().enumerate() {
        *b ^= mask[i % 16] ^ (i as u8);
    }
}

impl SngFile {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(path: &Path) -> Result<SngFile, String> {
        let file = File::open(path).map_err(|e| format!("cannot open: {e}"))?;
        let mut r = BufReader::new(file);
        let (metadata, files, xor_mask) = parse_header(&mut r)?;
        Ok(SngFile { metadata, files, path: path.to_path_buf(), xor_mask })
    }

    /// Open a container held fully in memory (the browser demo's fetched
    /// song). Reads slice straight into the shared buffer.
    #[cfg(target_arch = "wasm32")]
    pub fn from_bytes(data: std::sync::Arc<Vec<u8>>) -> Result<SngFile, String> {
        let mut r = std::io::Cursor::new(data.as_slice());
        let (metadata, files, xor_mask) = parse_header(&mut r)?;
        Ok(SngFile { metadata, files, data, xor_mask })
    }

    pub fn file_names(&self) -> impl Iterator<Item = &String> {
        self.files.keys()
    }

    pub fn has(&self, name: &str) -> bool {
        self.files.contains_key(&name.to_lowercase())
    }

    /// Read and unmask one contained file, seeking straight to its bytes.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read(&self, name: &str) -> Result<Vec<u8>, String> {
        let &(offset, len) = self
            .files
            .get(&name.to_lowercase())
            .ok_or_else(|| format!("{name}: not in archive"))?;
        let mut f = File::open(&self.path).map_err(|e| format!("cannot open: {e}"))?;
        let size = f.metadata().map_err(|e| e.to_string())?.len();
        if offset.checked_add(len).is_none_or(|end| end > size) {
            return Err(format!("{name}: index points past end of file"));
        }
        f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let mut raw = vec![0u8; len as usize];
        f.read_exact(&mut raw).map_err(|_| format!("{name}: truncated data"))?;
        unmask(&mut raw, &self.xor_mask);
        Ok(raw)
    }

    /// Read and unmask one contained file out of the in-memory buffer.
    #[cfg(target_arch = "wasm32")]
    pub fn read(&self, name: &str) -> Result<Vec<u8>, String> {
        let &(offset, len) = self
            .files
            .get(&name.to_lowercase())
            .ok_or_else(|| format!("{name}: not in archive"))?;
        let end = offset
            .checked_add(len)
            .filter(|&end| end <= self.data.len() as u64)
            .ok_or_else(|| format!("{name}: index points past end of file"))?;
        let mut raw = self.data[offset as usize..end as usize].to_vec();
        unmask(&mut raw, &self.xor_mask);
        Ok(raw)
    }
}
