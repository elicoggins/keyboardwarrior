// Native reader for the Clone Hero .sng container (SNGPKG v1): header with a
// 16-byte XOR mask, ini-style metadata pairs, a file index, then masked file
// data. A Chorus Encore download dropped into songs/ plays with no unpacking.
//
// Only the header, metadata, and file index are parsed at open time; contained
// files are read on demand by seeking into the data section. Scanning a large
// library therefore never pulls whole song archives (audio included) into
// memory just to list titles.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub struct SngFile {
    pub metadata: Vec<(String, String)>,
    files: HashMap<String, (u64, u64)>, // name -> (offset, len)
    path: PathBuf,
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

impl SngFile {
    pub fn open(path: &Path) -> Result<SngFile, String> {
        let file = File::open(path).map_err(|e| format!("cannot open: {e}"))?;
        let mut r = BufReader::new(file);

        let mut magic = [0u8; 6];
        fill(&mut r, &mut magic)?;
        if &magic != b"SNGPKG" {
            return Err("not an SNGPKG container".into());
        }
        let mut version = [0u8; 4];
        fill(&mut r, &mut version)?;
        let mut xor_mask = [0u8; 16];
        fill(&mut r, &mut xor_mask)?;

        let _meta_len = u64_of(&mut r)?;
        let meta_count = u64_of(&mut r)?;
        let mut metadata = Vec::new();
        for _ in 0..meta_count {
            let klen = i32_of(&mut r)?.max(0) as usize;
            let key = string_of(&mut r, klen)?;
            let vlen = i32_of(&mut r)?.max(0) as usize;
            let val = string_of(&mut r, vlen)?;
            metadata.push((key, val));
        }

        let _index_len = u64_of(&mut r)?;
        let file_count = u64_of(&mut r)?;
        let mut files = HashMap::new();
        for _ in 0..file_count {
            let nlen = u8_of(&mut r)? as usize;
            let name = string_of(&mut r, nlen)?;
            let len = u64_of(&mut r)?;
            let offset = u64_of(&mut r)?;
            files.insert(name.to_lowercase(), (offset, len));
        }

        Ok(SngFile { metadata, files, path: path.to_path_buf(), xor_mask })
    }

    pub fn file_names(&self) -> impl Iterator<Item = &String> {
        self.files.keys()
    }

    pub fn has(&self, name: &str) -> bool {
        self.files.contains_key(&name.to_lowercase())
    }

    /// Read and unmask one contained file, seeking straight to its bytes.
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
        for (i, b) in raw.iter_mut().enumerate() {
            *b ^= self.xor_mask[i % 16] ^ (i as u8);
        }
        Ok(raw)
    }
}
