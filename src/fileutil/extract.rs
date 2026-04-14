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
