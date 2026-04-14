use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// 计算文件 SHA256
pub fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// 校验 SHA256 是否匹配
pub fn verify_sha256(path: &Path, expected: &str) -> bool {
    sha256_file(path).map(|h| h == expected).unwrap_or(false)
}
