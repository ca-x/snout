use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserDataBehavior {
    Preserve,
    Discard,
}

fn should_keep_existing(
    path: &Path,
    user_data_behavior: UserDataBehavior,
    exclude_patterns: &[config::ExcludePattern],
) -> bool {
    if matches!(user_data_behavior, UserDataBehavior::Discard) {
        return false;
    }

    config::matches_any_exclude_pattern(path, exclude_patterns)
}

/// 解压 ZIP 文件到目标目录
pub fn extract_zip(
    zip_path: &Path,
    dest: &Path,
    user_data_behavior: UserDataBehavior,
    exclude_patterns: &[String],
) -> anyhow::Result<()> {
    let staging = stage_zip(zip_path)?;
    merge_directory(staging.path(), dest, user_data_behavior, exclude_patterns)
}

pub(crate) fn stage_zip(zip_path: &Path) -> anyhow::Result<tempfile::TempDir> {
    let staging = tempfile::Builder::new()
        .prefix("snout-extract-")
        .tempdir()?;
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        anyhow::ensure!(
            !entry.is_symlink(),
            "archive contains a symbolic link: {}",
            entry.name()
        );
        let outpath = match entry.enclosed_name() {
            Some(name) => staging.path().join(name),
            None => continue,
        };

        if entry.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            let parent = outpath.parent().context("archive entry has no parent")?;
            std::fs::create_dir_all(parent)?;
            let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
            std::io::copy(&mut entry, &mut temporary)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;

                if let Some(mode) = entry.unix_mode() {
                    temporary
                        .as_file()
                        .set_permissions(std::fs::Permissions::from_mode(mode & 0o777))?;
                }
            }
            temporary.persist(&outpath).map_err(|error| error.error)?;
        }
    }

    Ok(staging)
}

enum InstallEntry {
    Directory(PathBuf),
    File {
        source: PathBuf,
        destination: PathBuf,
    },
}

fn existing_metadata(path: &Path) -> anyhow::Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("inspect {}", path.display())),
    }
}

fn resolve_destination_root(destination: &Path) -> anyhow::Result<PathBuf> {
    let mut ancestor = destination;
    let mut missing = Vec::new();
    loop {
        if existing_metadata(ancestor)?.is_some() {
            let mut root = std::fs::canonicalize(ancestor)?;
            anyhow::ensure!(
                root.is_dir(),
                "destination root is not a directory: {}",
                root.display()
            );
            for name in missing.into_iter().rev() {
                root.push(name);
            }
            return Ok(root);
        }
        missing.push(
            ancestor
                .file_name()
                .context("destination root has no filename")?
                .to_os_string(),
        );
        ancestor = ancestor
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
    }
}

fn plan_merge(
    source: &Path,
    destination: &Path,
    user_data_behavior: UserDataBehavior,
    exclude_patterns: &[config::ExcludePattern],
    entries: &mut Vec<InstallEntry>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source = entry.path();
        let file_type = entry.file_type()?;
        anyhow::ensure!(
            !file_type.is_symlink(),
            "refusing source symbolic link: {}",
            source.display()
        );
        let target = destination.join(entry.file_name());
        let existing = existing_metadata(&target)?;
        if existing.is_some() && should_keep_existing(&target, user_data_behavior, exclude_patterns)
        {
            continue;
        }
        if let Some(metadata) = existing {
            anyhow::ensure!(
                !metadata.file_type().is_symlink(),
                "refusing destination symbolic link: {}",
                target.display()
            );
            anyhow::ensure!(
                (file_type.is_dir() && metadata.is_dir())
                    || (file_type.is_file() && metadata.is_file()),
                "incompatible destination: {}",
                target.display()
            );
        }
        if file_type.is_dir() {
            entries.push(InstallEntry::Directory(target.clone()));
            plan_merge(
                &source,
                &target,
                user_data_behavior,
                exclude_patterns,
                entries,
            )?;
        } else {
            anyhow::ensure!(
                file_type.is_file(),
                "unsupported source entry: {}",
                source.display()
            );
            entries.push(InstallEntry::File {
                source,
                destination: target,
            });
        }
    }
    Ok(())
}

fn reject_child_symlinks(root: &Path, destination: &Path) -> anyhow::Result<()> {
    let mut current = root.to_path_buf();
    for component in destination.strip_prefix(root)?.components() {
        current.push(component);
        if let Some(metadata) = existing_metadata(&current)? {
            anyhow::ensure!(
                !metadata.file_type().is_symlink(),
                "refusing destination symbolic link: {}",
                current.display()
            );
        }
    }
    Ok(())
}

fn install_file(root: &Path, source: &Path, destination: &Path) -> anyhow::Result<()> {
    reject_child_symlinks(root, destination)?;
    let mut source_file = std::fs::File::open(source)?;
    let permissions = match existing_metadata(destination)? {
        Some(metadata) => {
            anyhow::ensure!(
                metadata.is_file(),
                "incompatible destination: {}",
                destination.display()
            );
            metadata.permissions()
        }
        None => source_file.metadata()?.permissions(),
    };
    let parent = destination.parent().context("destination has no parent")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut source_file, &mut temporary)?;
    temporary.as_file().set_permissions(permissions)?;
    temporary.as_file().sync_all()?;
    reject_child_symlinks(root, destination)?;
    temporary
        .persist(destination)
        .map_err(|error| error.error)?;
    Ok(())
}

/// Merges staged files without replacing directories or following child symlinks.
/// Existing excluded files and directories are retained under the preserve policy.
/// The destination root may itself be a symlink and is canonicalized before merging.
pub fn merge_directory(
    source: &Path,
    destination: &Path,
    user_data_behavior: UserDataBehavior,
    exclude_patterns: &[String],
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !std::fs::symlink_metadata(source)?.file_type().is_symlink(),
        "refusing source symbolic link: {}",
        source.display()
    );
    let source = std::fs::canonicalize(source)?;
    let root = resolve_destination_root(destination)?;
    anyhow::ensure!(
        !root.starts_with(&source),
        "destination must not be inside the source directory"
    );
    let mut entries = Vec::new();
    let (exclude_patterns, _) = config::parse_exclude_patterns(exclude_patterns);
    plan_merge(
        &source,
        &root,
        user_data_behavior,
        &exclude_patterns,
        &mut entries,
    )?;
    std::fs::create_dir_all(&root)?;
    for entry in entries {
        match entry {
            InstallEntry::Directory(destination) => {
                reject_child_symlinks(&root, &destination)?;
                match existing_metadata(&destination)? {
                    Some(metadata) => anyhow::ensure!(
                        metadata.is_dir(),
                        "incompatible destination: {}",
                        destination.display()
                    ),
                    None => std::fs::create_dir(&destination)?,
                }
            }
            InstallEntry::File {
                source,
                destination,
            } => install_file(&root, &source, &destination)?,
        }
    }
    Ok(())
}

/// 处理 CNB 镜像的嵌套目录 (解压后可能多一层目录)
pub fn handle_nested_dir(
    base: &Path,
    _zip_name: &str,
    user_data_behavior: UserDataBehavior,
    exclude_patterns: &[String],
) -> anyhow::Result<()> {
    let root = std::fs::canonicalize(base)?;
    let mut wrappers = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        if path.join("lua").is_dir() || path.join("dicts").is_dir() {
            wrappers.push(path);
        }
    }
    anyhow::ensure!(
        wrappers.len() <= 1,
        "ambiguous archive wrapper directories in {}",
        root.display()
    );
    if let Some(wrapper) = wrappers.pop() {
        let staging = tempfile::Builder::new()
            .prefix(".snout-normalize-")
            .tempdir_in(&root)?;
        let incoming = staging.path().join("incoming");
        std::fs::rename(&wrapper, &incoming)?;
        if let Err(error) = merge_directory(&incoming, &root, user_data_behavior, exclude_patterns)
        {
            if let Err(restore_error) = std::fs::rename(&incoming, &wrapper) {
                let retained = staging.keep();
                return Err(error.context(format!(
                    "restore wrapper failed: {restore_error}; staged data retained at {}",
                    retained.display()
                )));
            }
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_test_zip(dir: &std::path::Path, files: &[(&str, &[u8])]) -> std::path::PathBuf {
        let zip_path = dir.join("test.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        for (name, content) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(content).unwrap();
        }
        zip.finish().unwrap();
        zip_path
    }

    #[cfg(unix)]
    fn assert_symlink_destination_rejected(entry_name: &str, link_name: &str) {
        let directory = tempfile::tempdir().unwrap();
        let outside = directory.path().join("outside");
        let dest = directory.path().join("output");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        let victim = outside.join("victim.txt");
        std::fs::write(&victim, b"outside-original").unwrap();
        let target = if link_name == "parent" {
            &outside
        } else {
            &victim
        };
        std::os::unix::fs::symlink(target, dest.join(link_name)).unwrap();
        let archive = create_test_zip(directory.path(), &[(entry_name, b"archive-replacement")]);

        let result = extract_zip(&archive, &dest, UserDataBehavior::Discard, &[]);

        assert_eq!(std::fs::read(&victim).unwrap(), b"outside-original");
        assert!(result.is_err());
        assert!(dest.join(link_name).is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn regression_extract_rejects_symlinked_parent() {
        assert_symlink_destination_rejected("parent/victim.txt", "parent");
    }

    #[cfg(unix)]
    #[test]
    fn regression_extract_rejects_symlinked_final_file() {
        assert_symlink_destination_rejected("victim.txt", "victim.txt");
    }

    #[cfg(unix)]
    #[test]
    fn extract_allows_canonical_destination_root_symlink() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("actual-root");
        let dest = directory.path().join("linked-root");
        std::fs::create_dir(&target).unwrap();
        std::os::unix::fs::symlink(&target, &dest).unwrap();
        let archive = create_test_zip(directory.path(), &[("lua/new.lua", b"new-lua")]);

        extract_zip(&archive, &dest, UserDataBehavior::Preserve, &[]).unwrap();

        assert!(dest.is_symlink());
        assert_eq!(
            std::fs::read(target.join("lua/new.lua")).unwrap(),
            b"new-lua"
        );
    }

    #[cfg(unix)]
    #[test]
    fn extract_atomically_replaces_hard_link_without_truncating_original() {
        use std::io::Read;
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("output");
        std::fs::create_dir(&dest).unwrap();
        let original = directory.path().join("original");
        std::fs::write(&original, b"original-content").unwrap();
        std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o640)).unwrap();
        std::fs::hard_link(&original, dest.join("asset.txt")).unwrap();
        let mut snapshot = std::fs::File::open(dest.join("asset.txt")).unwrap();
        let archive = create_test_zip(directory.path(), &[("asset.txt", b"new-content")]);

        extract_zip(&archive, &dest, UserDataBehavior::Discard, &[]).unwrap();

        let mut previous = Vec::new();
        snapshot.read_to_end(&mut previous).unwrap();
        assert_eq!(previous, b"original-content");
        assert_eq!(std::fs::read(&original).unwrap(), b"original-content");
        assert_eq!(
            std::fs::read(dest.join("asset.txt")).unwrap(),
            b"new-content"
        );
        assert_eq!(
            std::fs::metadata(dest.join("asset.txt"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 1);
    }

    #[test]
    fn corrupt_archive_leaves_live_files_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("output");
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("first.txt"), b"original-first").unwrap();
        let archive = create_test_zip(
            directory.path(),
            &[
                ("first.txt", b"replacement-first"),
                ("broken.txt", b"broken-payload"),
            ],
        );
        let mut bytes = std::fs::read(&archive).unwrap();
        let payload = bytes
            .windows(b"broken-payload".len())
            .position(|bytes| bytes == b"broken-payload")
            .unwrap();
        bytes[payload] ^= 1;
        std::fs::write(&archive, bytes).unwrap();

        assert!(extract_zip(&archive, &dest, UserDataBehavior::Discard, &[]).is_err());

        assert_eq!(
            std::fs::read(dest.join("first.txt")).unwrap(),
            b"original-first"
        );
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn merge_rejects_all_symlink_conflicts_before_installing_files() {
        let directory = tempfile::tempdir().unwrap();
        let incoming = directory.path().join("incoming");
        let dest = directory.path().join("output");
        std::fs::create_dir(&incoming).unwrap();
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(incoming.join("first.txt"), b"new-first").unwrap();
        std::fs::write(incoming.join("blocked.txt"), b"new-blocked").unwrap();
        std::os::unix::fs::symlink(directory.path().join("missing"), dest.join("blocked.txt"))
            .unwrap();

        assert!(merge_directory(&incoming, &dest, UserDataBehavior::Discard, &[]).is_err());

        assert!(!dest.join("first.txt").exists());
        assert!(dest.join("blocked.txt").is_symlink());
        assert!(!directory.path().join("missing").exists());
    }

    #[cfg(unix)]
    #[test]
    fn regression_invalid_source_does_not_create_destination_root() {
        let directory = tempfile::tempdir().unwrap();
        let incoming = directory.path().join("incoming");
        std::fs::create_dir(&incoming).unwrap();
        std::os::unix::fs::symlink(directory.path().join("outside"), incoming.join("bad-link"))
            .unwrap();
        let dest = directory.path().join("missing/deep/output");

        assert!(merge_directory(&incoming, &dest, UserDataBehavior::Discard, &[]).is_err());

        assert!(!directory.path().join("missing").exists());
    }

    #[test]
    fn regression_merge_into_source_does_not_create_destination_root() {
        let incoming = tempfile::tempdir().unwrap();
        std::fs::write(incoming.path().join("asset.txt"), b"original").unwrap();
        let dest = incoming.path().join("new/output");

        assert!(merge_directory(incoming.path(), &dest, UserDataBehavior::Discard, &[]).is_err());

        assert!(!incoming.path().join("new").exists());
        assert_eq!(
            std::fs::read(incoming.path().join("asset.txt")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn archive_symlink_is_rejected_before_live_installation() {
        let directory = tempfile::tempdir().unwrap();
        let archive_path = directory.path().join("symlink.zip");
        let mut archive = zip::ZipWriter::new(std::fs::File::create(&archive_path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        archive.start_file("first.txt", options).unwrap();
        archive.write_all(b"new-first").unwrap();
        archive.add_symlink("link", "outside", options).unwrap();
        archive.finish().unwrap();
        let dest = directory.path().join("output");

        assert!(extract_zip(&archive_path, &dest, UserDataBehavior::Discard, &[]).is_err());

        assert!(!dest.exists());
    }

    #[test]
    fn regression_nested_wrapper_merges_populated_root_per_file() {
        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("output");
        std::fs::create_dir_all(dest.join("lua")).unwrap();
        std::fs::create_dir_all(dest.join("terra.userdb")).unwrap();
        for (relative, contents) in [
            ("lua/user.lua", "user-lua"),
            ("lua/keep.lua", "user-excluded-lua"),
            ("squirrel.custom.yaml", "user-custom"),
            ("terra.userdb/state", "user-learning"),
            ("wanxiang.gram", "user-model"),
        ] {
            std::fs::write(dest.join(relative), contents).unwrap();
        }
        let archive = create_test_zip(
            directory.path(),
            &[
                ("wrapper/lua/new.lua", b"new-lua"),
                ("wrapper/lua/keep.lua", b"archive-excluded-lua"),
                ("wrapper/squirrel.custom.yaml", b"archive-custom"),
                ("wrapper/terra.userdb/state", b"archive-learning"),
                ("wrapper/schema.yaml", b"new-schema"),
            ],
        );
        let mut exclusions = config::default_exclude_patterns();
        exclusions.push("keep.lua".into());

        extract_zip(&archive, &dest, UserDataBehavior::Preserve, &exclusions).unwrap();
        handle_nested_dir(&dest, "test.zip", UserDataBehavior::Preserve, &exclusions).unwrap();

        assert_eq!(
            std::fs::read(dest.join("schema.yaml")).unwrap(),
            b"new-schema"
        );
        assert_eq!(std::fs::read(dest.join("lua/new.lua")).unwrap(), b"new-lua");
        for (relative, contents) in [
            ("lua/user.lua", "user-lua"),
            ("lua/keep.lua", "user-excluded-lua"),
            ("squirrel.custom.yaml", "user-custom"),
            ("terra.userdb/state", "user-learning"),
            ("wanxiang.gram", "user-model"),
        ] {
            assert_eq!(
                std::fs::read_to_string(dest.join(relative)).unwrap(),
                contents
            );
        }
        assert!(!dest.join("wrapper").exists());
    }

    #[cfg(unix)]
    #[test]
    fn failed_normalization_restores_wrapper_and_preserves_outside_target() {
        let directory = tempfile::tempdir().unwrap();
        let dest = directory.path().join("output");
        let wrapper = dest.join("wrapper");
        let outside = directory.path().join("outside");
        std::fs::create_dir_all(wrapper.join("lua")).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(wrapper.join("lua/new.lua"), b"new-lua").unwrap();
        std::fs::write(outside.join("new.lua"), b"outside-original").unwrap();
        std::os::unix::fs::symlink(&outside, dest.join("lua")).unwrap();

        assert!(handle_nested_dir(&dest, "test.zip", UserDataBehavior::Discard, &[]).is_err());

        assert_eq!(
            std::fs::read(outside.join("new.lua")).unwrap(),
            b"outside-original"
        );
        assert_eq!(
            std::fs::read(wrapper.join("lua/new.lua")).unwrap(),
            b"new-lua"
        );
        assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn normalization_source_error_restores_original_wrapper() {
        let directory = tempfile::tempdir().unwrap();
        let wrapper = directory.path().join("wrapper");
        std::fs::create_dir_all(wrapper.join("lua")).unwrap();
        std::fs::write(wrapper.join("lua/module.lua"), b"module").unwrap();
        std::os::unix::fs::symlink("missing-target", wrapper.join("unsafe")).unwrap();

        assert!(
            handle_nested_dir(directory.path(), "test.zip", UserDataBehavior::Discard, &[])
                .is_err()
        );

        assert_eq!(
            std::fs::read(wrapper.join("lua/module.lua")).unwrap(),
            b"module"
        );
        assert!(wrapper.join("unsafe").is_symlink());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn ambiguous_wrappers_fail_without_moving_existing_files() {
        let directory = tempfile::tempdir().unwrap();
        for wrapper in ["first", "second"] {
            std::fs::create_dir_all(directory.path().join(wrapper).join("lua")).unwrap();
            std::fs::write(
                directory.path().join(wrapper).join("lua/module.lua"),
                b"module",
            )
            .unwrap();
        }

        assert!(
            handle_nested_dir(directory.path(), "test.zip", UserDataBehavior::Discard, &[])
                .is_err()
        );

        for wrapper in ["first", "second"] {
            assert_eq!(
                std::fs::read(directory.path().join(wrapper).join("lua/module.lua")).unwrap(),
                b"module"
            );
        }
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
    }

    #[test]
    fn test_extract_simple_zip() {
        let tmp = std::env::temp_dir().join("snout-test-extract");
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp).unwrap();
        }
        std::fs::create_dir_all(&tmp).unwrap();

        let zip_path = create_test_zip(
            &tmp,
            &[
                ("hello.txt", b"hello world"),
                ("subdir/nested.txt", b"nested content"),
            ],
        );

        let dest = tmp.join("output");
        extract_zip(
            &zip_path,
            &dest,
            UserDataBehavior::Preserve,
            &config::default_exclude_patterns(),
        )
        .unwrap();

        let hello = std::fs::read_to_string(dest.join("hello.txt")).unwrap();
        assert_eq!(hello, "hello world");

        let nested = std::fs::read_to_string(dest.join("subdir/nested.txt")).unwrap();
        assert_eq!(nested, "nested content");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_handle_nested_dir() {
        let tmp = std::env::temp_dir().join("snout-test-nested");
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp).unwrap();
        }
        std::fs::create_dir_all(&tmp).unwrap();

        let wrapper = tmp.join("wrapper");
        std::fs::create_dir_all(wrapper.join("lua")).unwrap();
        std::fs::write(wrapper.join("lua/test.lua"), "test").unwrap();
        std::fs::write(wrapper.join("schema.yaml"), "schema").unwrap();

        handle_nested_dir(
            &tmp,
            "test.zip",
            UserDataBehavior::Preserve,
            &config::default_exclude_patterns(),
        )
        .unwrap();

        assert!(tmp.join("lua/test.lua").exists());
        assert!(tmp.join("schema.yaml").exists());
        assert!(!wrapper.exists());

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_extract_zip_preserves_existing_custom_files() {
        let tmp = std::env::temp_dir().join("snout-test-preserve-custom");
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp).unwrap();
        }
        std::fs::create_dir_all(&tmp).unwrap();

        let zip_path = create_test_zip(&tmp, &[("weasel.custom.yaml", b"archive")]);
        let dest = tmp.join("output");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("weasel.custom.yaml"), "user").unwrap();

        extract_zip(
            &zip_path,
            &dest,
            UserDataBehavior::Preserve,
            &config::default_exclude_patterns(),
        )
        .unwrap();

        let content = std::fs::read_to_string(dest.join("weasel.custom.yaml")).unwrap();
        assert_eq!(content, "user");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_handle_nested_dir_preserves_existing_custom_files() {
        let tmp = std::env::temp_dir().join("snout-test-nested-preserve-custom");
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp).unwrap();
        }
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("squirrel.custom.yaml"), "user").unwrap();

        let wrapper = tmp.join("wrapper");
        std::fs::create_dir_all(&wrapper).unwrap();
        std::fs::create_dir_all(wrapper.join("lua")).unwrap();
        std::fs::write(wrapper.join("lua/test.lua"), "test").unwrap();
        std::fs::write(wrapper.join("squirrel.custom.yaml"), "archive").unwrap();
        std::fs::write(wrapper.join("schema.yaml"), "schema").unwrap();

        handle_nested_dir(
            &tmp,
            "test.zip",
            UserDataBehavior::Preserve,
            &config::default_exclude_patterns(),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(tmp.join("squirrel.custom.yaml")).unwrap(),
            "user"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.join("schema.yaml")).unwrap(),
            "schema"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_extract_zip_preserves_user_learning_data_files() {
        let tmp = std::env::temp_dir().join("snout-test-preserve-userdb");
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp).unwrap();
        }
        std::fs::create_dir_all(&tmp).unwrap();

        let zip_path = create_test_zip(
            &tmp,
            &[
                ("terra_pinyin.userdb.txt", b"archive-userdb"),
                ("custom_phrase.txt", b"archive-phrases"),
            ],
        );
        let dest = tmp.join("output");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("terra_pinyin.userdb.txt"), "user-userdb").unwrap();
        std::fs::write(dest.join("custom_phrase.txt"), "user-phrases").unwrap();

        extract_zip(
            &zip_path,
            &dest,
            UserDataBehavior::Preserve,
            &config::default_exclude_patterns(),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(dest.join("terra_pinyin.userdb.txt")).unwrap(),
            "user-userdb"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("custom_phrase.txt")).unwrap(),
            "user-phrases"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_extract_zip_can_discard_existing_user_learning_data_files() {
        let tmp = std::env::temp_dir().join("snout-test-discard-userdb");
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp).unwrap();
        }
        std::fs::create_dir_all(&tmp).unwrap();

        let zip_path = create_test_zip(
            &tmp,
            &[
                ("terra_pinyin.userdb.txt", b"archive-userdb"),
                ("custom_phrase.txt", b"archive-phrases"),
            ],
        );
        let dest = tmp.join("output");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("terra_pinyin.userdb.txt"), "user-userdb").unwrap();
        std::fs::write(dest.join("custom_phrase.txt"), "user-phrases").unwrap();

        extract_zip(
            &zip_path,
            &dest,
            UserDataBehavior::Discard,
            &config::default_exclude_patterns(),
        )
        .unwrap();

        assert_eq!(
            std::fs::read_to_string(dest.join("terra_pinyin.userdb.txt")).unwrap(),
            "archive-userdb"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("custom_phrase.txt")).unwrap(),
            "archive-phrases"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }
}
