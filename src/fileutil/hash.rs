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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_sha256_known_value() {
        // SHA256 of "hello world" (without newline)
        let dir = std::env::temp_dir().join("snout-test-hash");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.txt");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"hello world").unwrap();
        drop(f);

        let hash = sha256_file(&path).unwrap();
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );

        assert!(verify_sha256(&path, &hash));
        assert!(!verify_sha256(&path, "deadbeef"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_sha256_empty_file() {
        let dir = std::env::temp_dir().join("snout-test-hash-empty");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.txt");
        std::fs::File::create(&path).unwrap();

        let hash = sha256_file(&path).unwrap();
        // SHA256 of empty string
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_verify_nonexistent_file() {
        assert!(!verify_sha256(Path::new("/nonexistent/file"), "abc"));
    }
}
