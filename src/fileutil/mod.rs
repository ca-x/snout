pub mod extract;
pub mod hash;

pub fn atomic_write(path: &std::path::Path, contents: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;

    let resolved;
    let path = if path.is_symlink() {
        resolved = std::fs::canonicalize(path)?;
        resolved.as_path()
    } else {
        path
    };
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(contents)?;
    match std::fs::metadata(path) {
        Ok(metadata) => temporary
            .as_file()
            .set_permissions(metadata.permissions())?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::atomic_write;

    #[test]
    fn atomic_write_creates_and_replaces_without_leaving_temporary_files() {
        let root = tempfile::tempdir().expect("temporary root");
        let path = root.path().join("nested/config.json");
        atomic_write(&path, b"original").expect("first write");
        atomic_write(&path, b"replacement").expect("replace");
        assert_eq!(
            std::fs::read(&path).expect("saved contents"),
            b"replacement"
        );
        assert_eq!(
            std::fs::read_dir(path.parent().expect("parent"))
                .expect("directory")
                .count(),
            1
        );
    }

    #[test]
    fn failed_replacement_preserves_destination_and_cleans_temporary_files() {
        let root = tempfile::tempdir().expect("temporary root");
        let destination = root.path().join("existing-directory");
        std::fs::create_dir(&destination).expect("destination");
        std::fs::write(destination.join("important.txt"), b"keep").expect("existing data");
        assert!(atomic_write(&destination, b"replacement").is_err());
        assert_eq!(
            std::fs::read(destination.join("important.txt")).expect("preserved data"),
            b"keep"
        );
        assert_eq!(std::fs::read_dir(root.path()).expect("root").count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_keeps_symlinks_and_existing_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("temporary root");
        let destination = root.path().join("target");
        let link = root.path().join("link");
        std::fs::write(&destination, b"original").expect("target");
        std::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o640))
            .expect("target permissions");
        std::os::unix::fs::symlink(&destination, &link).expect("symlink");
        atomic_write(&link, b"replacement").expect("write through symlink");
        assert!(link.is_symlink());
        assert_eq!(
            std::fs::read(destination).expect("contents"),
            b"replacement"
        );
        assert_eq!(
            std::fs::metadata(link)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
}
