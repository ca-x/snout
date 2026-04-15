# snout 实现进度

## ✅ 已完成

### Phase 1-2: 核心框架 (2512 行 Rust)
- [x] `types.rs` — Schema 枚举 (万象10变体 + 雾凇 + 白霜), Config, UpdateInfo/Record, GitHub API 类型
- [x] `config.rs` — 跨平台配置管理, Rime 用户目录自动检测
- [x] `api/mod.rs` — GitHub API + CNB 镜像客户端, SOCKS5/HTTP 代理支持
- [x] `fileutil/hash.rs` — SHA256 校验
- [x] `fileutil/extract.rs` — ZIP 解压 + CNB 嵌套目录处理
- [x] `updater/mod.rs` — BaseUpdater + SchemeUpdater + DictUpdater + ModelUpdater + 组合更新
- [x] `updater/model_patch.rs` — YAML patch 语法模型 (grammar/language_model)
- [x] `deployer/mod.rs` — 跨平台部署 (小狼毫/鼠须管/Fcitx5)
- [x] `skin/builtin.rs` — 6个内置主题 (简纯/Win11浅暗/微信/Mac白/灵梦)
- [x] `skin/patch.rs` — YAML patch 写入 (preset_color_schemes)
- [x] `main.rs` — CLI: --init, --update, --scheme, --dict, --model, --patch-model, --mirror, --proxy

### Phase 3: TUI 框架
- [x] `ui/app.rs` — ratatui TUI, 主菜单, 键盘导航 (↑↓/jk/Enter/Esc)
- [x] `ui/wizard.rs` — 首次初始化向导
- [x] 方案选择器 (12个方案)
- [x] 皮肤选择器 (6个内置主题)
- [x] 更新进度条 (Gauge)
- [x] 结果展示屏幕
- [x] 配置信息查看
- [x] 通知系统

## 待实现

### Phase 4: 部署完善
- [ ] Fcitx 兼容目录同步 (Linux)
- [ ] 多引擎同步 (同时更新到 fcitx5 + ibus)
- [ ] Pre/Post update hooks

### Phase 5: 测试
- [ ] GitHub API 端到端测试 (需要网络)
- [ ] 实际下载 + 解压 + patch 流程测试
- [ ] 各方案 zip 文件名验证

### Phase 6: 打包
- [ ] release build (cargo build --release)
- [ ] PKGBUILD (Arch Linux AUR)
- [ ] Cross-compile (Windows/macOS/Linux)

## 文件结构
```
snout/src/
├── main.rs           (CLI 入口)
├── types.rs          (核心类型定义)
├── config.rs         (配置管理)
├── api/mod.rs        (GitHub/CNB 客户端)
├── fileutil/
│   ├── mod.rs
│   ├── hash.rs       (SHA256)
│   └── extract.rs    (ZIP 解压)
├── updater/
│   ├── mod.rs        (更新器核心)
│   └── model_patch.rs (模型 patch)
├── deployer/mod.rs   (跨平台部署)
├── skin/
│   ├── mod.rs
│   ├── builtin.rs    (内置主题)
│   └── patch.rs      (YAML patch)
└── ui/
    ├── mod.rs
    ├── app.rs        (ratatui TUI)
    ├── wizard.rs     (初始化向导)
    └── update_view.rs (预留)
```

## 方案源信息
| 方案 | 仓库 | 方案 zip | 词库 zip |
|------|------|----------|----------|
| 万象 (10变体) | amzxyz/rime_wanxiang | rime-wanxiang-{base,moqi,flypy,...}.zip | base-dicts.zip / pro-dicts.zip |
| 雾凇 | iDvel/rime-ice | full.zip | all_dicts.zip |
| 白霜 | gaboolic/rime-frost | rime-frost-schemas.zip | 内嵌 |
| 模型 | amzxyz/RIME-LMDG | wanxiang-lts-zh-hans.gram | - |
