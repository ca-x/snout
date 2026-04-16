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
        zh.insert("menu.done", "完成");
        en.insert("menu.done", "Done");
        zh.insert("menu.result", "结果");
        en.insert("menu.result", "Result");
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
        zh.insert("menu.fcitx5_theme", "Fcitx5 主题");
        en.insert("menu.fcitx5_theme", "Fcitx5 Theme");
        zh.insert("menu.switch_scheme", "切换方案");
        en.insert("menu.switch_scheme", "Switch Scheme");
        zh.insert("menu.config", "配置");
        en.insert("menu.config", "Config");
        zh.insert("menu.quit", "退出");
        en.insert("menu.quit", "Quit");
        zh.insert("menu.action_ready", "可执行");
        en.insert("menu.action_ready", "Ready");
        zh.insert(
            "menu.desc.update_all",
            "检查并更新方案、词库、模型，然后触发部署。",
        );
        en.insert(
            "menu.desc.update_all",
            "Check and update scheme, dictionary, and model, then deploy.",
        );
        zh.insert("menu.desc.update_scheme", "只更新当前选中方案。");
        en.insert(
            "menu.desc.update_scheme",
            "Update the currently selected scheme only.",
        );
        zh.insert("menu.desc.update_dict", "只更新当前方案的独立词库。");
        en.insert(
            "menu.desc.update_dict",
            "Update the current scheme's standalone dictionary only.",
        );
        zh.insert("menu.desc.update_model", "下载或更新语言模型文件。");
        en.insert(
            "menu.desc.update_model",
            "Download or update the language model file.",
        );
        zh.insert("menu.desc.model_patch", "为当前方案启用或移除模型 patch。");
        en.insert(
            "menu.desc.model_patch",
            "Enable or remove the model patch for the current scheme.",
        );
        zh.insert("menu.desc.skin_patch", "设置输入法主题或皮肤 patch。");
        en.insert(
            "menu.desc.skin_patch",
            "Set the input method theme or skin patch.",
        );
        zh.insert("menu.desc.switch_scheme", "切换当前使用的输入方案。");
        en.insert("menu.desc.switch_scheme", "Switch the active input scheme.");
        zh.insert("menu.desc.config", "查看并调整运行时配置。");
        en.insert("menu.desc.config", "View and adjust runtime configuration.");
        zh.insert("menu.desc.quit", "退出程序。");
        en.insert("menu.desc.quit", "Exit the program.");

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
        zh.insert("wizard.install_one_of", "请先安装:");
        en.insert("wizard.install_one_of", "Install one of:");
        zh.insert("wizard.install.weasel", "小狼毫 - Windows");
        en.insert("wizard.install.weasel", "Weasel - Windows");
        zh.insert("wizard.install.squirrel", "鼠须管 - macOS");
        en.insert("wizard.install.squirrel", "Squirrel - macOS");
        zh.insert("wizard.install.fcitx5", "Fcitx5 + Rime - Linux");
        en.insert("wizard.install.fcitx5", "Fcitx5 + Rime - Linux");
        zh.insert("wizard.open_tui", "运行 `snout` 打开 TUI");
        en.insert("wizard.open_tui", "Run `snout` to open TUI");
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
        zh.insert("update.partial", "更新完成，但有问题");
        en.insert("update.partial", "Update completed with issues");
        zh.insert("update.cancelling", "正在取消...");
        en.insert("update.cancelling", "Cancelling...");
        zh.insert("update.cancelled", "更新已取消");
        en.insert("update.cancelled", "Update cancelled");
        zh.insert("update.up_to_date", "已是最新版本");
        en.insert("update.up_to_date", "Already up to date");
        zh.insert("status.not_installed", "未安装");
        en.insert("status.not_installed", "Not installed");
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
        zh.insert("deploy.reloaded.fcitx5", "Fcitx5 已重载");
        en.insert("deploy.reloaded.fcitx5", "Fcitx5 reloaded");
        zh.insert("deploy.reloaded.ibus", "IBus 已重载");
        en.insert("deploy.reloaded.ibus", "IBus reloaded");
        zh.insert("deploy.reloaded.squirrel", "鼠须管已重载");
        en.insert("deploy.reloaded.squirrel", "Squirrel reloaded");
        zh.insert("deploy.reloaded.weasel", "小狼毫已重载");
        en.insert("deploy.reloaded.weasel", "Weasel reloaded");
        zh.insert("deploy.target_failed", "部署到目标引擎失败");
        en.insert("deploy.target_failed", "Failed to deploy to target engine");
        zh.insert("deploy.sync_partial_failed", "部分引擎同步失败");
        en.insert("deploy.sync_partial_failed", "Partial engine sync failed");
        zh.insert("deploy.symlink_created", "已创建软链接");
        en.insert("deploy.symlink_created", "Symlink created");
        zh.insert("deploy.synced_to", "已同步到");
        en.insert("deploy.synced_to", "Synced to");
        zh.insert("deploy.hook_missing", "hook 不存在");
        en.insert("deploy.hook_missing", "Hook does not exist");
        zh.insert("deploy.hook_running", "执行 hook");
        en.insert("deploy.hook_running", "Running hook");
        zh.insert("deploy.hook_failed", "hook 执行失败");
        en.insert("deploy.hook_failed", "Hook execution failed");
        zh.insert("deploy.binary_not_found", "未找到可执行文件");
        en.insert("deploy.binary_not_found", "Binary not found");
        zh.insert("deploy.no_engine_detected", "未检测到 Rime 引擎");
        en.insert("deploy.no_engine_detected", "No Rime engine detected");
        zh.insert("deploy.all_engines_failed", "所有 Rime 引擎部署失败");
        en.insert(
            "deploy.all_engines_failed",
            "All Rime engine deployments failed",
        );
        zh.insert("deploy.partial_engines_failed", "部分引擎部署失败");
        en.insert(
            "deploy.partial_engines_failed",
            "Some engine deployments failed",
        );

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
        zh.insert("update.status_section", "当前步骤");
        en.insert("update.status_section", "Current steps");
        zh.insert("update.deploying", "部署...");
        en.insert("update.deploying", "Deploying...");
        zh.insert("update.syncing", "同步引擎目录...");
        en.insert("update.syncing", "Syncing engine directories...");
        zh.insert("update.nested_dir_failed", "嵌套目录处理失败");
        en.insert(
            "update.nested_dir_failed",
            "Nested directory handling failed",
        );
        zh.insert("update.component.model_patch", "模型 Patch");
        en.insert("update.component.model_patch", "Model Patch");
        zh.insert("update.component.deploy", "部署");
        en.insert("update.component.deploy", "Deploy");
        zh.insert("update.component.hook", "Hook");
        en.insert("update.component.hook", "Hook");
        zh.insert("update.component.sync", "引擎同步");
        en.insert("update.component.sync", "Engine Sync");

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
        zh.insert(
            "patch.model.section_invalid",
            "模型 patch 文件中的 patch 节不是映射类型",
        );
        en.insert(
            "patch.model.section_invalid",
            "The patch section in the model patch file is not a mapping",
        );
        zh.insert("patch.model.written", "模型 patch 已写入");
        en.insert("patch.model.written", "Model patch written");
        zh.insert("patch.model.removed", "模型 patch 已移除");
        en.insert("patch.model.removed", "Model patch removed");
        zh.insert("patch.model.status_read_failed", "读取模型 patch 状态失败");
        en.insert(
            "patch.model.status_read_failed",
            "Failed to read model patch status",
        );
        zh.insert("patch.model.read_failed", "读取模型 patch 文件失败");
        en.insert("patch.model.read_failed", "Failed to read model patch file");
        zh.insert("patch.model.parse_failed", "解析模型 patch 文件失败");
        en.insert(
            "patch.model.parse_failed",
            "Failed to parse model patch file",
        );

        // ── 皮肤 ──
        zh.insert("skin.select", "选择皮肤");
        en.insert("skin.select", "Select skin");
        zh.insert("skin.select_prompt", "选择皮肤 (Enter确认/Esc返回)");
        en.insert(
            "skin.select_prompt",
            "Choose skin (Enter confirm / Esc back)",
        );
        zh.insert(
            "skin.fcitx5_select_prompt",
            "选择 Fcitx5 主题 (Enter确认/Esc返回)",
        );
        en.insert(
            "skin.fcitx5_select_prompt",
            "Choose Fcitx5 theme (Enter confirm / Esc back)",
        );
        zh.insert("skin.applied", "皮肤已设置");
        en.insert("skin.applied", "Skin applied");
        zh.insert("skin.not_supported", "当前平台不支持皮肤 Patch");
        en.insert(
            "skin.not_supported",
            "Skin patch is not supported on this platform",
        );
        zh.insert("skin.installed_marker", "已安装");
        en.insert("skin.installed_marker", "Installed");
        zh.insert("skin.current_marker", "当前");
        en.insert("skin.current_marker", "Current");

        // ── 方案 ──
        zh.insert("scheme.select", "选择方案");
        en.insert("scheme.select", "Select scheme");
        zh.insert("scheme.select_prompt", "选择方案 (Enter确认/Esc返回)");
        en.insert(
            "scheme.select_prompt",
            "Choose scheme (Enter confirm / Esc back)",
        );
        zh.insert("scheme.switched", "方案已切换");
        en.insert("scheme.switched", "Scheme switched");
        zh.insert("schema.wanxiang_base", "万象拼音 (标准版)");
        en.insert("schema.wanxiang_base", "Wanxiang (Base)");
        zh.insert("schema.wanxiang_moqi", "万象拼音 Pro (墨奇辅助)");
        en.insert("schema.wanxiang_moqi", "Wanxiang Pro (Moqi)");
        zh.insert("schema.wanxiang_flypy", "万象拼音 Pro (小鹤辅助)");
        en.insert("schema.wanxiang_flypy", "Wanxiang Pro (Flypy)");
        zh.insert("schema.wanxiang_zrm", "万象拼音 Pro (自然码辅助)");
        en.insert("schema.wanxiang_zrm", "Wanxiang Pro (Ziranma)");
        zh.insert("schema.wanxiang_tiger", "万象拼音 Pro (虎码辅助)");
        en.insert("schema.wanxiang_tiger", "Wanxiang Pro (Tiger Code)");
        zh.insert("schema.wanxiang_wubi", "万象拼音 Pro (五笔辅助)");
        en.insert("schema.wanxiang_wubi", "Wanxiang Pro (Wubi)");
        zh.insert("schema.wanxiang_hanxin", "万象拼音 Pro (汉心辅助)");
        en.insert("schema.wanxiang_hanxin", "Wanxiang Pro (Hanxin)");
        zh.insert("schema.wanxiang_shouyou", "万象拼音 Pro (首右辅助)");
        en.insert("schema.wanxiang_shouyou", "Wanxiang Pro (Shouyou)");
        zh.insert("schema.wanxiang_shyplus", "万象拼音 Pro (首右+辅助)");
        en.insert("schema.wanxiang_shyplus", "Wanxiang Pro (Shouyou+)");
        zh.insert("schema.wanxiang_wx", "万象拼音 Pro (万象辅助)");
        en.insert("schema.wanxiang_wx", "Wanxiang Pro (Wanxiang)");
        zh.insert("schema.ice", "雾凇拼音");
        en.insert("schema.ice", "Rime Ice");
        zh.insert("schema.frost", "白霜拼音");
        en.insert("schema.frost", "Rime Frost");
        zh.insert("schema.mint", "薄荷输入法");
        en.insert("schema.mint", "Mint Input");
        zh.insert("schema.unknown", "未知方案");
        en.insert("schema.unknown", "Unknown schema");

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

        // ── API / 网络 ──
        zh.insert("api.proxy_unknown", "未知代理类型");
        en.insert("api.proxy_unknown", "Unknown proxy type");
        zh.insert("api.github_branch_status", "GitHub Branch API 返回");
        en.insert("api.github_branch_status", "GitHub Branch API returned");
        zh.insert(
            "api.github_branch_missing_sha",
            "GitHub Branch API 缺少 commit.sha",
        );
        en.insert(
            "api.github_branch_missing_sha",
            "GitHub Branch API is missing commit.sha",
        );
        zh.insert("api.github_status", "GitHub API 返回");
        en.insert("api.github_status", "GitHub API returned");
        zh.insert("api.github_request_failed", "GitHub API 请求失败");
        en.insert("api.github_request_failed", "GitHub API request failed");
        zh.insert("api.cnb_status", "CNB API 返回");
        en.insert("api.cnb_status", "CNB API returned");
        zh.insert("api.cnb_request_failed", "CNB API 请求失败");
        en.insert("api.cnb_request_failed", "CNB API request failed");
        zh.insert("api.cnb_no_release", "CNB 无 release");
        en.insert("api.cnb_no_release", "CNB has no release");
        zh.insert("api.download_retry", "下载失败，稍后重试");
        en.insert("api.download_retry", "Download failed, retrying shortly");
        zh.insert("api.download_request_failed", "下载请求失败");
        en.insert("api.download_request_failed", "Download request failed");
        zh.insert("api.download_http_failed", "下载失败: HTTP");
        en.insert("api.download_http_failed", "Download failed: HTTP");
        zh.insert("api.download_interrupted", "下载中断");
        en.insert("api.download_interrupted", "Download interrupted");

        // ── 提示 ──
        zh.insert("hint.navigate", "导航");
        en.insert("hint.navigate", "Navigate");
        zh.insert("hint.confirm", "确认");
        en.insert("hint.confirm", "Confirm");
        zh.insert("hint.back", "返回/退出");
        en.insert("hint.back", "Back/Quit");
        zh.insert("hint.input", "输入");
        en.insert("hint.input", "Input");
        zh.insert("hint.notice", "提示");
        en.insert("hint.notice", "Notice");
        zh.insert("hint.wait", "处理中，请等待");
        en.insert("hint.wait", "Working, please wait");
        zh.insert("hint.cancel", "取消");
        en.insert("hint.cancel", "Cancel");
        zh.insert("hint.unavailable", "当前不可用");
        en.insert("hint.unavailable", "Currently unavailable");
        zh.insert("hint.toggle", "切换");
        en.insert("hint.toggle", "Toggle");
        zh.insert("hint.refresh", "刷新");
        en.insert("hint.refresh", "Refresh");

        // ── 配置 ──
        zh.insert("config.current_scheme", "当前方案");
        en.insert("config.current_scheme", "Current scheme");
        zh.insert("config.detected_engines", "检测到的引擎");
        en.insert("config.detected_engines", "Detected engines");
        zh.insert("config.language_label", "语言");
        en.insert("config.language_label", "Language");
        zh.insert("config.mirror_label", "镜像下载");
        en.insert("config.mirror_label", "Mirror downloads");
        zh.insert("config.proxy_label", "代理");
        en.insert("config.proxy_label", "Proxy");
        zh.insert("config.proxy_source_label", "代理来源");
        en.insert("config.proxy_source_label", "Proxy source");
        zh.insert("config.proxy_value_label", "代理地址");
        en.insert("config.proxy_value_label", "Proxy address");
        zh.insert("config.proxy_type_label", "代理类型");
        en.insert("config.proxy_type_label", "Proxy type");
        zh.insert("config.proxy_source_config", "配置文件");
        en.insert("config.proxy_source_config", "Config");
        zh.insert("config.proxy_source_env", "环境变量");
        en.insert("config.proxy_source_env", "Environment");
        zh.insert(
            "config.proxy_env_readonly",
            "环境变量代理仅展示，需修改环境变量本身",
        );
        en.insert(
            "config.proxy_env_readonly",
            "Environment proxy is read-only here; change the environment variable itself",
        );
        zh.insert("config.proxy_type_http", "HTTP");
        en.insert("config.proxy_type_http", "HTTP");
        zh.insert("config.proxy_type_socks5", "SOCKS5");
        en.insert("config.proxy_type_socks5", "SOCKS5");
        zh.insert("config.edit_title", "编辑配置");
        en.insert("config.edit_title", "Edit config");
        zh.insert("config.input_placeholder", "输入内容");
        en.insert("config.input_placeholder", "Enter value");
        zh.insert("config.input_hint", "输入新值后按 Enter 保存");
        en.insert(
            "config.input_hint",
            "Enter a new value and press Enter to save",
        );
        zh.insert("config.model_patch_label", "自动模型 Patch");
        en.insert("config.model_patch_label", "Auto model patch");
        zh.insert("config.engine_sync_label", "多引擎同步");
        en.insert("config.engine_sync_label", "Multi-engine sync");
        zh.insert("config.sync_strategy_label", "同步方式");
        en.insert("config.sync_strategy_label", "Sync strategy");
        zh.insert("config.runtime_section", "当前状态");
        en.insert("config.runtime_section", "Current setup");
        zh.insert("config.features_section", "启用特性");
        en.insert("config.features_section", "Enabled features");
        zh.insert("config.paths_section", "路径");
        en.insert("config.paths_section", "Paths");
        zh.insert("config.status_section", "更新状态");
        en.insert("config.status_section", "Update status");
        zh.insert("config.scheme_status_label", "方案状态");
        en.insert("config.scheme_status_label", "Scheme status");
        zh.insert("config.dict_status_label", "词库状态");
        en.insert("config.dict_status_label", "Dictionary status");
        zh.insert("config.model_status_label", "模型状态");
        en.insert("config.model_status_label", "Model status");
        zh.insert("config.model_patch_status_label", "模型 Patch 状态");
        en.insert("config.model_patch_status_label", "Model patch status");
        zh.insert("config.enabled", "开启");
        en.insert("config.enabled", "Enabled");
        zh.insert("config.disabled", "关闭");
        en.insert("config.disabled", "Disabled");
        zh.insert("config.none", "无");
        en.insert("config.none", "None");
        zh.insert("config.loading", "读取中");
        en.insert("config.loading", "Loading");
        zh.insert("config.sync_link", "软链接");
        en.insert("config.sync_link", "Symlink");
        zh.insert("config.sync_copy", "复制");
        en.insert("config.sync_copy", "Copy");
        zh.insert("config.latest", "最新");
        en.insert("config.latest", "Latest");
        zh.insert("config.unknown", "未知");
        en.insert("config.unknown", "Unknown");
        zh.insert("config.saved", "配置已保存");
        en.insert("config.saved", "Configuration saved");
        zh.insert("config.na", "不适用");
        en.insert("config.na", "N/A");
        zh.insert("config.lang.zh", "中文");
        en.insert("config.lang.zh", "Chinese");
        zh.insert("config.lang.en", "英文");
        en.insert("config.lang.en", "English");
        zh.insert("config.rime_dir", "Rime 目录");
        en.insert("config.rime_dir", "Rime directory");
        zh.insert("config.config_file", "配置文件");
        en.insert("config.config_file", "Config file");
        zh.insert("config.supported_schemes", "支持方案");
        en.insert("config.supported_schemes", "Supported schemes");
        zh.insert("config.scheme.wanxiang", "万象拼音: amzxyz/rime_wanxiang");
        en.insert("config.scheme.wanxiang", "Wanxiang: amzxyz/rime_wanxiang");
        zh.insert("config.scheme.ice", "雾凇拼音: iDvel/rime-ice");
        en.insert("config.scheme.ice", "Rime Ice: iDvel/rime-ice");
        zh.insert("config.scheme.frost", "白霜拼音: gaboolic/rime-frost");
        en.insert("config.scheme.frost", "Rime Frost: gaboolic/rime-frost");
        zh.insert("config.scheme.mint", "薄荷输入法: Mintimate/oh-my-rime");
        en.insert("config.scheme.mint", "Mint Input: Mintimate/oh-my-rime");
        zh.insert("skin.jianchun", "简纯");
        en.insert("skin.jianchun", "Jianchun");
        zh.insert("skin.win11_light", "Win11浅色");
        en.insert("skin.win11_light", "Win11 Light");
        zh.insert("skin.win11_dark", "Win11暗色");
        en.insert("skin.win11_dark", "Win11 Dark");
        zh.insert("skin.wechat", "微信");
        en.insert("skin.wechat", "WeChat");
        zh.insert("skin.mac_light", "Mac 白");
        en.insert("skin.mac_light", "Mac Light");
        zh.insert("skin.lumk_light", "鹿鸣 / Lumk light");
        en.insert("skin.lumk_light", "Lumk light");
        zh.insert("skin.amber_7", "淡白 / weasel");
        en.insert("skin.amber_7", "Amber-7 / weasel");
        zh.insert("skin.win10gray", "win10灰 / win10gray");
        en.insert("skin.win10gray", "win10gray");
        zh.insert("skin.mint_light_blue", "蓝水鸭 / Mint Light Blue");
        en.insert("skin.mint_light_blue", "Mint Light Blue");
        zh.insert("skin.ayaya", "文文 / Ayaya");
        en.insert("skin.ayaya", "Ayaya");
        zh.insert("skin.ayaya_dark", "文文 / Ayaya / 深色");
        en.insert("skin.ayaya_dark", "Ayaya Dark");
        zh.insert("skin.reimu", "灵梦");
        en.insert("skin.reimu", "Reimu");
        zh.insert("skin.reimu_dark", "灵梦 / Reimu / 深色");
        en.insert("skin.reimu_dark", "Reimu Dark");
        zh.insert("skin.apathy", "冷漠 / Apathy");
        en.insert("skin.apathy", "Apathy");
        zh.insert("skin.win10", "WIN10");
        en.insert("skin.win10", "WIN10");
        zh.insert("skin.win10_ayaya", "WIN10 / 文文 / Ayaya");
        en.insert("skin.win10_ayaya", "WIN10 / Ayaya");
        zh.insert("skin.macos12_light", "高仿亮色 macOS");
        en.insert("skin.macos12_light", "macOS 12 Light");
        zh.insert("skin.macos12_dark", "高仿暗色 macOS");
        en.insert("skin.macos12_dark", "macOS 12 Dark");
        zh.insert("skin.wechat_dark", "高仿暗色微信输入法");
        en.insert("skin.wechat_dark", "WeChat Dark");
        zh.insert("skin.macos14", "高仿 macOS 14");
        en.insert("skin.macos14", "macOS 14");
        zh.insert("skin.macos14_dark", "高仿暗色 macOS 14");
        en.insert("skin.macos14_dark", "macOS 14 Dark");
        zh.insert("config.title", "配置信息");
        en.insert("config.title", "Config Info");
        zh.insert("config.back", "按 Esc 返回");
        en.insert("config.back", "Press Esc to return");
        zh.insert(
            "config.parse_failed_defaulting",
            "配置文件解析失败，使用默认配置",
        );
        en.insert(
            "config.parse_failed_defaulting",
            "Failed to parse config file, using defaults",
        );
        zh.insert("config.appdata_missing", "APPDATA 未设置");
        en.insert("config.appdata_missing", "APPDATA is not set");
        zh.insert("config.home_missing", "无法获取 HOME");
        en.insert("config.home_missing", "Failed to resolve HOME");
        zh.insert("config.config_dir_missing", "无法获取 config 目录");
        en.insert(
            "config.config_dir_missing",
            "Failed to resolve config directory",
        );
        zh.insert("result.back_to_menu", "按 Enter 返回主菜单");
        en.insert("result.back_to_menu", "Press Enter to return to menu");

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
