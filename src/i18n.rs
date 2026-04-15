use std::collections::HashMap;

/// 支持的语言
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh,
    En,
}

impl Lang {
    pub fn from_str(s: &str) -> Self {
        if s.starts_with("zh") || s.starts_with("cn") {
            Lang::Zh
        } else {
            Lang::En
        }
    }
}

/// 消息 key
pub struct L10n {
    lang: Lang,
    zh: HashMap<&'static str, &'static str>,
    en: HashMap<&'static str, &'static str>,
}

impl L10n {
    pub fn new(lang: Lang) -> Self {
        let mut zh = HashMap::new();
        let mut en = HashMap::new();

        // ── 通用 ──
        zh.insert("app.name", "snout");
        en.insert("app.name", "snout");
        zh.insert("app.desc", "Rime 输入法初始化与更新工具");
        en.insert("app.desc", "Rime Input Method Init & Update Tool");

        // ── 菜单 ──
        zh.insert("menu.title", "主菜单");
        en.insert("menu.title", "Main Menu");
        zh.insert("menu.update_all", "一键更新");
        en.insert("menu.update_all", "Update All");
        zh.insert("menu.update_scheme", "更新方案");
        en.insert("menu.update_scheme", "Update Scheme");
        zh.insert("menu.update_dict", "更新词库");
        en.insert("menu.update_dict", "Update Dictionary");
        zh.insert("menu.update_model", "更新模型");
        en.insert("menu.update_model", "Update Model");
        zh.insert("menu.model_patch", "模型 Patch");
        en.insert("menu.model_patch", "Model Patch");
        zh.insert("menu.skin_patch", "皮肤 Patch");
        en.insert("menu.skin_patch", "Skin Patch");
        zh.insert("menu.switch_scheme", "切换方案");
        en.insert("menu.switch_scheme", "Switch Scheme");
        zh.insert("menu.config", "配置");
        en.insert("menu.config", "Config");
        zh.insert("menu.quit", "退出");
        en.insert("menu.quit", "Quit");

        // ── 向导 ──
        zh.insert("wizard.title", "首次初始化向导");
        en.insert("wizard.title", "First-time Setup Wizard");
        zh.insert("wizard.no_engine", "未检测到已安装的 Rime 输入法引擎");
        en.insert("wizard.no_engine", "No Rime input method engine detected");
        zh.insert("wizard.engine_found", "检测到引擎");
        en.insert("wizard.engine_found", "Detected engine");
        zh.insert("wizard.select_scheme", "选择方案");
        en.insert("wizard.select_scheme", "Select scheme");
        zh.insert("wizard.enable_model_patch", "启用语言模型 patch?");
        en.insert("wizard.enable_model_patch", "Enable language model patch?");
        zh.insert("wizard.downloading", "下载安装中...");
        en.insert("wizard.downloading", "Downloading and installing...");
        zh.insert("wizard.complete", "初始化完成");
        en.insert("wizard.complete", "Setup complete");

        // ── 更新 ──
        zh.insert("update.checking", "检查更新...");
        en.insert("update.checking", "Checking for updates...");
        zh.insert("update.downloading", "下载中");
        en.insert("update.downloading", "Downloading");
        zh.insert("update.verifying", "校验文件...");
        en.insert("update.verifying", "Verifying...");
        zh.insert("update.extracting", "解压中...");
        en.insert("update.extracting", "Extracting...");
        zh.insert("update.deploying", "部署中...");
        en.insert("update.deploying", "Deploying...");
        zh.insert("update.complete", "更新完成");
        en.insert("update.complete", "Update complete");
        zh.insert("update.up_to_date", "已是最新版本");
        en.insert("update.up_to_date", "Already up to date");
        zh.insert("update.failed", "更新失败");
        en.insert("update.failed", "Update failed");
        zh.insert("update.scheme", "方案");
        en.insert("update.scheme", "Scheme");
        zh.insert("update.dict", "词库");
        en.insert("update.dict", "Dictionary");
        zh.insert("update.model", "模型");
        en.insert("update.model", "Model");
        zh.insert("update.model_not_supported", "此方案不支持模型更新");
        en.insert(
            "update.model_not_supported",
            "This scheme does not support model updates",
        );

        // ── 部署 ──
        zh.insert("deploy.reloading", "正在重载");
        en.insert("deploy.reloading", "Reloading");
        zh.insert("deploy.complete", "Rime 已重载");
        en.insert("deploy.complete", "Rime reloaded");
        zh.insert("deploy.failed", "部署失败");
        en.insert("deploy.failed", "Deploy failed");

        // ── 更新进度 ──
        zh.insert("update.scheme.checking", "检查方案更新...");
        en.insert("update.scheme.checking", "Checking scheme updates...");
        zh.insert("update.dict.checking", "检查词库更新...");
        en.insert("update.dict.checking", "Checking dict updates...");
        zh.insert("update.model.checking", "检查模型更新...");
        en.insert("update.model.checking", "Checking model updates...");
        zh.insert("update.cache.verify", "校验本地缓存...");
        en.insert("update.cache.verify", "Verifying local cache...");
        zh.insert("update.cache.valid", "缓存有效，跳过下载");
        en.insert("update.cache.valid", "Cache valid, skipping download");
        zh.insert("update.key_file_missing", "关键文件缺失，强制更新...");
        en.insert(
            "update.key_file_missing",
            "Key file missing, forcing update...",
        );
        zh.insert("update.scheme_switched", "检测到方案切换，重新下载...");
        en.insert(
            "update.scheme_switched",
            "Scheme switched, re-downloading...",
        );
        zh.insert("update.saving", "保存记录...");
        en.insert("update.saving", "Saving record...");
        zh.insert("update.download_scheme", "下载方案...");
        en.insert("update.download_scheme", "Downloading scheme...");
        zh.insert("update.download_dict", "下载词库...");
        en.insert("update.download_dict", "Downloading dict...");
        zh.insert("update.download_model", "下载模型...");
        en.insert("update.download_model", "Downloading model...");
        zh.insert("update.download_progress", "下载中...");
        en.insert("update.download_progress", "Downloading...");
        zh.insert("update.scheme_done", "方案更新完成");
        en.insert("update.scheme_done", "Scheme update complete");
        zh.insert("update.mint_scheme_checking", "检查薄荷方案更新...");
        en.insert(
            "update.mint_scheme_checking",
            "Checking Mint scheme updates...",
        );
        zh.insert("update.mint_scheme_done", "薄荷方案更新完成");
        en.insert("update.mint_scheme_done", "Mint scheme update complete");
        zh.insert("update.dict_done", "词库更新完成");
        en.insert("update.dict_done", "Dict update complete");
        zh.insert("update.model_done", "模型更新完成");
        en.insert("update.model_done", "Model update complete");
        zh.insert("update.no_dict", "此方案无独立词库");
        en.insert("update.no_dict", "This scheme has no separate dict");
        zh.insert("update.progress", "进度");
        en.insert("update.progress", "Progress");
        zh.insert("update.deploying", "部署...");
        en.insert("update.deploying", "Deploying...");

        // ── 部署 ──

        // ── Model Patch ──
        zh.insert("patch.model.enabled", "模型 patch 已启用");
        en.insert("patch.model.enabled", "Model patch enabled");
        zh.insert("patch.model.disabled", "模型 patch 已移除");
        en.insert("patch.model.disabled", "Model patch removed");
        zh.insert("patch.model.not_supported", "此方案不支持模型 patch");
        en.insert(
            "patch.model.not_supported",
            "This scheme does not support model patch",
        );

        // ── 皮肤 ──
        zh.insert("skin.select", "选择皮肤");
        en.insert("skin.select", "Select skin");
        zh.insert("skin.applied", "皮肤已设置");
        en.insert("skin.applied", "Skin applied");
        zh.insert("skin.not_supported", "当前平台不支持皮肤 Patch");
        en.insert(
            "skin.not_supported",
            "Skin patch is not supported on this platform",
        );

        // ── 方案 ──
        zh.insert("scheme.select", "选择方案");
        en.insert("scheme.select", "Select scheme");
        zh.insert("scheme.switched", "方案已切换");
        en.insert("scheme.switched", "Scheme switched");

        // ── Hook ──
        zh.insert("hook.pre_update", "执行 pre-update hook");
        en.insert("hook.pre_update", "Running pre-update hook");
        zh.insert("hook.post_update", "执行 post-update hook");
        en.insert("hook.post_update", "Running post-update hook");

        // ── Fcitx ──
        zh.insert("fcitx.syncing", "同步 Fcitx 目录");
        en.insert("fcitx.syncing", "Syncing Fcitx directory");

        // ── 错误 ──
        zh.insert("err.download_failed", "下载失败");
        en.insert("err.download_failed", "Download failed");
        zh.insert("err.sha256_mismatch", "SHA256 校验失败");
        en.insert("err.sha256_mismatch", "SHA256 verification failed");
        zh.insert("err.extract_failed", "解压失败");
        en.insert("err.extract_failed", "Extraction failed");
        zh.insert("err.no_scheme_file", "未找到方案文件");
        en.insert("err.no_scheme_file", "Scheme file not found");
        zh.insert("err.no_dict_file", "未找到词库文件");
        en.insert("err.no_dict_file", "Dictionary file not found");
        zh.insert("err.no_model_file", "未找到模型文件");
        en.insert("err.no_model_file", "Model file not found");

        // ── 提示 ──
        zh.insert("hint.navigate", "导航");
        en.insert("hint.navigate", "Navigate");
        zh.insert("hint.confirm", "确认");
        en.insert("hint.confirm", "Confirm");
        zh.insert("hint.back", "返回/退出");
        en.insert("hint.back", "Back/Quit");

        // ── 配置 ──
        zh.insert("config.current_scheme", "当前方案");
        en.insert("config.current_scheme", "Current scheme");
        zh.insert("config.rime_dir", "Rime 目录");
        en.insert("config.rime_dir", "Rime directory");
        zh.insert("config.config_file", "配置文件");
        en.insert("config.config_file", "Config file");

        Self { lang, zh, en }
    }

    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        let map = match self.lang {
            Lang::Zh => &self.zh,
            Lang::En => &self.en,
        };
        map.get(key).copied().unwrap_or(key)
    }

    #[allow(dead_code)]
    pub fn lang(&self) -> Lang {
        self.lang
    }
}
