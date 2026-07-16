// Native reader for the Clone Hero .sng container (SNGPKG v1): header with a
// 16-byte XOR mask, ini-style metadata pairs, a file index, then masked file
// data. A Chorus Encore download dropped into songs/ plays with no unpacking.

use std::collections::HashMap;
use std::path::Path;

pub struct SngFile {
    pub metadata: Vec<(String, String)>,
    files: HashMap<String, (u64, u64)>, // name -> (offset, len)
    data: Vec<u8>,
    xor_mask: [u8; 16],
}

impl SngFile {
    pub fn open(path: &Path) -> Option<SngFile> {
        let data = std::fs::read(path).ok()?;
        let mut p = 0usize;
        let take = |p: &mut usize, n: usize| -> Option<&[u8]> {
            let s = data.get(*p..*p + n)?;
            *p += n;
            Some(s)
        };
        let u32_at = |p: &mut usize| -> Option<u32> {
            Some(u32::from_le_bytes(take(p, 4)?.try_into().ok()?))
        };
        let u64_at = |p: &mut usize| -> Option<u64> {
            Some(u64::from_le_bytes(take(p, 8)?.try_into().ok()?))
        };
        let i32_at = |p: &mut usize| -> Option<i32> {
            Some(i32::from_le_bytes(take(p, 4)?.try_into().ok()?))
        };

        if take(&mut p, 6)? != b"SNGPKG" {
            return None;
        }
        let _version = u32_at(&mut p)?;
        let xor_mask: [u8; 16] = take(&mut p, 16)?.try_into().ok()?;

        let _meta_len = u64_at(&mut p)?;
        let meta_count = u64_at(&mut p)?;
        let mut metadata = Vec::new();
        for _ in 0..meta_count {
            let klen = i32_at(&mut p)?.max(0) as usize;
            let key = String::from_utf8_lossy(take(&mut p, klen)?).to_string();
            let vlen = i32_at(&mut p)?.max(0) as usize;
            let val = String::from_utf8_lossy(take(&mut p, vlen)?).to_string();
            metadata.push((key, val));
        }

        let _index_len = u64_at(&mut p)?;
        let file_count = u64_at(&mut p)?;
        let mut files = HashMap::new();
        for _ in 0..file_count {
            let nlen = take(&mut p, 1)?[0] as usize;
            let name = String::from_utf8_lossy(take(&mut p, nlen)?).to_string();
            let len = u64_at(&mut p)?;
            let offset = u64_at(&mut p)?;
            files.insert(name.to_lowercase(), (offset, len));
        }

        Some(SngFile { metadata, files, data, xor_mask })
    }

    pub fn file_names(&self) -> impl Iterator<Item = &String> {
        self.files.keys()
    }

    /// Read and unmask one contained file.
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        let &(offset, len) = self.files.get(&name.to_lowercase())?;
        let raw = self.data.get(offset as usize..(offset + len) as usize)?;
        Some(
            raw.iter()
                .enumerate()
                .map(|(i, &b)| b ^ self.xor_mask[i % 16] ^ (i as u8))
                .collect(),
        )
    }

}
