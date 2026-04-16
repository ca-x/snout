use std::path::Path;

/// 解压 ZIP 文件到目标目录
pub fn extract_zip(zip_path: &Path, dest: &Path) -> anyhow::Result<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    std::fs::create_dir_all(dest)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let outpath = match entry.enclosed_name() {
            Some(name) => dest.join(name),
            None => continue,
        };

        if entry.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            if outpath.exists() && should_preserve_existing(&outpath) {
                continue;
            }
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut outfile = std::fs::File::create(&outpath)?;
            std::io::copy(&mut entry, &mut outfile)?;
        }

        // 保留权限
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = entry.unix_mode() {
                std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode))?;
            }
        }
    }

    Ok(())
}

/// 处理 CNB 镜像的嵌套目录 (解压后可能多一层目录)
pub fn handle_nested_dir(base: &Path, _zip_name: &str) -> anyhow::Result<()> {
    let entries: Vec<_> = std::fs::read_dir(base)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();

    // 如果只有一个子目录且里面有 lua/dicts 等，就提升一层
    if entries.len() == 1 {
        let subdir = &entries[0].path();
        let has_lua = subdir.join("lua").exists();
        let has_dicts = subdir.join("dicts").exists();
        if has_lua || has_dicts {
            // 临时移到 base
            let tmp = base.join("__tmp_mv");
            std::fs::rename(subdir, &tmp)?;
            // 移动子目录内容到 base
            for entry in std::fs::read_dir(&tmp)? {
                let entry = entry?;
                let from = entry.path();
                let to = base.join(entry.file_name());
                if to.exists() {
                    if should_preserve_existing(&to) {
                        if from.is_dir() {
                            std::fs::remove_dir_all(&from)?;
                        } else {
                            std::fs::remove_file(&from)?;
                        }
                        continue;
                    }
                    if to.is_dir() {
                        std::fs::remove_dir_all(&to)?;
                    } else {
                        std::fs::remove_file(&to)?;
                    }
                }
                std::fs::rename(&from, &to)?;
            }
            std::fs::remove_dir_all(&tmp)?;
        }
    }

    Ok(())
}

fn should_preserve_existing(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    file_name.ends_with(".custom.yaml") || matches!(file_name, "installation.yaml" | "user.yaml")
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
        extract_zip(&zip_path, &dest).unwrap();

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

        handle_nested_dir(&tmp, "test.zip").unwrap();

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

        extract_zip(&zip_path, &dest).unwrap();

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

        handle_nested_dir(&tmp, "test.zip").unwrap();

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
}
