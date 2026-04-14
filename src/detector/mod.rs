use crate::config::detect_installed_engines;

/// 检测已安装的 Rime 引擎
pub fn detect_engines() -> Vec<String> {
    detect_installed_engines()
}

/// 检测 Rime 是否已安装
pub fn check_rime_installed() -> bool {
    !detect_engines().is_empty()
}
