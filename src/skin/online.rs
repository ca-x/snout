//! Import downloadable Fcitx5 themes without executing repository code.
use crate::api::ThemeRepository;
use crate::fileutil::extract::{merge_directory, stage_zip, UserDataBehavior};
use crate::types::CancelSignal;
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Import every ready-to-use theme in an archive into a repository-specific namespace.
/// Existing unrelated themes and the active theme selection are preserved.
pub fn import_archive(
    archive: &Path,
    root: &Path,
    repository: &ThemeRepository,
    cancel: &CancelSignal,
) -> Result<Vec<String>> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?)?;
    ensure!(
        zip.len() <= 10_000,
        "Too many files in theme archive / 主题包文件过多"
    );
    let mut expanded = 0u64;
    for index in 0..zip.len() {
        cancel.checkpoint()?;
        let entry = zip.by_index(index)?;
        expanded = expanded.saturating_add(entry.size());
        ensure!(
            expanded <= 256 * 1024 * 1024,
            "Expanded theme archive exceeds 256 MiB / 解压主题包过大"
        );
        ensure!(
            !entry.is_symlink()
                && entry
                    .enclosed_name()
                    .is_some_and(|path| path.components().count() <= 32),
            "Unsafe theme archive path / 主题包包含不安全路径"
        );
    }
    drop(zip);
    let staging = stage_zip(archive)?;
    let mut themes = Vec::new();
    discover(staging.path(), &mut themes, cancel)?;
    ensure!(!themes.is_empty(), "No ready-to-use theme.conf found. This repository may require generation; follow its README. / 未找到可直接使用的 theme.conf，该仓库可能需要生成主题，请参阅其 README。");
    let mut imported = Vec::new();
    for directory in themes {
        cancel.checkpoint()?;
        let relative = directory.strip_prefix(staging.path())?;
        // GitHub's outer archive folder changes with the branch; it is not theme identity.
        let relative: PathBuf = relative.components().skip(1).collect();
        let digest = format!(
            "{:x}",
            Sha256::digest(relative.to_string_lossy().as_bytes())
        );
        let key = format!("snout-online-{}-{}", repository.id, &digest[..12]);
        let destination = root.join(&key);
        // Validate required configuration before installing any files for this variant.
        let config = std::fs::read_to_string(directory.join("theme.conf"))
            .context("read online theme.conf")?;
        ensure!(
            config.lines().any(|line| line.trim() == "[Metadata]")
                && config.lines().any(|line| line.trim() == "[InputPanel]"),
            "Not a Fcitx5 Classic UI theme / 并非 Fcitx5 Classic UI 主题"
        );
        merge_directory(&directory, &destination, UserDataBehavior::Discard, &[])?;
        crate::fileutil::atomic_write(
            &destination.join(".snout-source.json"),
            &serde_json::to_vec(
                &serde_json::json!({"repository": repository.full_name, "name": relative.to_string_lossy()}),
            )?,
        )?;
        imported.push(key);
    }
    Ok(imported)
}

fn discover(directory: &Path, themes: &mut Vec<PathBuf>, cancel: &CancelSignal) -> Result<()> {
    cancel.checkpoint()?;
    if directory.join("theme.conf").is_file() {
        themes.push(directory.to_owned());
        return Ok(());
    }
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            discover(&entry.path(), themes, cancel)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn repository() -> ThemeRepository {
        ThemeRepository {
            id: 123,
            full_name: "owner/theme".into(),
            default_branch: "main".into(),
            description: None,
            topics: vec![],
            stargazers_count: 0,
        }
    }
    fn archive(entries: &[(&str, &str)]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(file.reopen().unwrap());
        for (name, content) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        file
    }
    #[test]
    fn imports_nested_variants_without_colliding_or_running_scripts() {
        let config = "[Metadata]\nName=Sample\n[InputPanel]\nNormalColor=#000000\n";
        let zip = archive(&[
            ("repo-main/light/theme.conf", config),
            ("repo-main/light/panel.svg", "svg"),
            ("repo-main/dark/theme.conf", config),
            ("repo-main/install.sh", "exit 1"),
        ]);
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("unrelated"), "keep").unwrap();
        let names =
            import_archive(zip.path(), root.path(), &repository(), &CancelSignal::new()).unwrap();
        assert_eq!(names.len(), 2);
        assert_ne!(names[0], names[1]);
        assert_eq!(
            std::fs::read_to_string(root.path().join("unrelated")).unwrap(),
            "keep"
        );
        assert!(!root.path().join("install.sh").exists());
        assert_eq!(
            import_archive(zip.path(), root.path(), &repository(), &CancelSignal::new())
                .unwrap()
                .len(),
            2
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 3);
    }
    #[test]
    fn rejects_missing_theme_and_traversal_before_writing() {
        for entries in [
            vec![("repo/README.md", "generate first")],
            vec![("../theme.conf", "bad")],
        ] {
            let zip = archive(&entries);
            let root = tempfile::tempdir().unwrap();
            assert!(
                import_archive(zip.path(), root.path(), &repository(), &CancelSignal::new())
                    .is_err()
            );
            assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
        }
    }
    #[test]
    fn cancelled_import_has_no_side_effects() {
        let zip = archive(&[("repo/theme/theme.conf", "[Metadata]\n[InputPanel]")]);
        let root = tempfile::tempdir().unwrap();
        let cancel = CancelSignal::new();
        cancel.cancel();
        assert!(import_archive(zip.path(), root.path(), &repository(), &cancel).is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}
