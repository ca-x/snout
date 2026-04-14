# rime-init - Rime 输入法初始化与更新工具 (Rust 版)

## 概述

Rust 重写的 rime-wanxiang-updater，在保留原 Go 版全部功能基础上，扩展支持 **薄雾**、**白霜**、**万象** 三大方案，并新增模型自动 patch 和皮肤 patch 功能。

参考代码在 `~/rime-wanxiang-updater/` (Go 版)，请参照其架构和功能实现 Rust 版。

## 仓库信息

- GitHub Owner: `amzxyz`
- 主仓库: `rime_wanxiang` (万象方案)
- 模型仓库: `RIME-LMDG` (tag: `LTS`，模型文件: `wanxiang-lts-zh-hans.gram`)
- CNB 镜像: `cnb.cool`，repo: `rime-wanxiang`

## 支持的方案 (Schema)

| 方案 | 方案 zip 名 | 词库 zip 名 | GitHub Release Tag |
|------|------------|------------|-------------------|
| 万象 (基础) | `wanxiang-xhup.zip` | `wanxiang-xhup-dicts.zip` | 各 release |
| 万象 (增强·墨奇) | `wanxiang-xhup-fuzhu.zip` | `wanxiang-xhup-dicts.zip` | 同上 |
| 万象 (增强·虎码) | `wanxiang-xhup-fuzhu-tiger.zip` | `wanxiang-xhup-dicts.zip` | 同上 |
| 万象 (增强·自然码) | `wanxiang-xhup-fuzhu-zrm.zip` | `wanxiang-xhup-dicts.zip` | 同上 |
| 万象 (增强·五笔) | `wanxiang-xhup-fuzhu-wubi.zip` | `wanxiang-xhup-dicts.zip` | 同上 |
| 万象 (增强·汉心) | `wanxiang-xhup-fuzhu-hanxin.zip` | `wanxiang-xhup-dicts.zip` | 同上 |
| 薄雾 (Misty) | `wanxiang-xhup-misty.zip` | `wanxiang-xhup-dicts.zip` | 同上 |
| 白霜 (Frost) | `wanxiang-xhup-frost.zip` | `wanxiang-xhup-dicts.zip` | 同上 |

> 薄雾和白霜的具体 zip 文件名，请在 GitHub Releases 中确认（搜索 `misty` 和 `frost`）。
> 如果薄雾/白霜没有独立的 zip，可能需要从主仓库中提取对应的子目录。

## 架构设计

```
rime-init/
├── Cargo.toml
├── src/
│   ├── main.rs            # 入口：CLI 参数解析 + TUI 启动
│   ├── config.rs          # 配置管理 (JSON，平台特定路径)
│   ├── types.rs           # 核心类型定义
│   ├── api/
│   │   ├── mod.rs         # API 客户端 (GitHub + CNB)
│   │   ├── github.rs      # GitHub Releases API
│   │   └── cnb.rs         # CNB 镜像 API
│   ├── updater/
│   │   ├── mod.rs         # 更新器 trait + 组合更新
│   │   ├── scheme.rs      # 方案更新器
│   │   ├── dict.rs        # 词库更新器
│   │   ├── model.rs       # 模型更新器
│   │   └── model_patch.rs # 模型 patch (新增功能)
│   ├── fileutil/
│   │   ├── mod.rs         # 文件操作工具
│   │   ├── download.rs    # 下载 (断点续传 + 进度)
│   │   ├── extract.rs     # ZIP 解压
│   │   └── hash.rs        # SHA256 校验
│   ├── deployer/
│   │   ├── mod.rs         # 部署 trait
│   │   ├── windows.rs     # 小狼毫
│   │   ├── macos.rs       # 鼠须管
│   │   └── linux.rs       # Fcitx5/IBus
│   ├── detector/
│   │   ├── mod.rs         # 引擎检测
│   │   ├── windows.rs
│   │   ├── macos.rs
│   │   └── linux.rs
│   ├── skin/
│   │   ├── mod.rs         # 皮肤/主题 patch (新增功能)
│   │   ├── builtin.rs     # 内置主题定义
│   │   └── patch.rs       # YAML patch 写入
│   └── ui/
│       ├── mod.rs         # TUI 入口
│       ├── main_menu.rs   # 主菜单
│       ├── config_menu.rs # 配置菜单
│       ├── update_view.rs # 更新进度视图
│       ├── theme_menu.rs  # 主题选择菜单
│       └── wizard.rs      # 首次运行向导
```

## 核心依赖

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
ratatui = "0.29"
crossterm = "0.28"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"          # YAML patch 读写
reqwest = { version = "0.12", features = ["json", "socks5", "stream"] }
tokio = { version = "1", features = ["full"] }
zip = "2"
sha2 = "0.10"
hex = "0.4"
indicatif = "0.17"
anyhow = "1"
dirs = "5"
chrono = { version = "0.4", features = ["serde"] }
futures-util = "0.3"
```

## 功能清单

### 1. 保留 Go 版全部功能
- [ ] 一键更新 (方案+词库+模型)
- [ ] 分项更新 (单独更新方案/词库/模型)
- [ ] 版本检查与对比 (SHA256 + tag)
- [ ] 断点续传下载
- [ ] 代理支持 (SOCKS5/HTTP)
- [ ] CNB 镜像加速
- [ ] 跨平台部署 (小狼毫/鼠须管/Fcitx5)
- [ ] 多引擎同步
- [ ] Pre/Post update hooks
- [ ] 排除文件管理
- [ ] TUI 界面 (键盘优先，ratatui)
- [ ] 中英双语界面
- [ ] 首次运行配置向导
- [ ] Fcitx 兼容目录同步 (Linux)

### 2. 新增: 多方案支持
- [ ] 支持薄雾 (Misty) 方案
- [ ] 支持白霜 (Frost) 方案
- [ ] 支持万象 (Wanxiang) 全部辅助码变体
- [ ] 首次运行或配置中可切换方案

### 3. 新增: 模型自动 Patch
当用户选择万象方案时，提供选项自动下载并 patch 语言模型:
- [ ] 从 `RIME-LMDG` 仓库 (tag `LTS`) 下载 `wanxiang-lts-zh-hans.gram`
- [ ] 放置到 Rime 用户目录
- [ ] 通过修改 `wanxiang.schema.yaml` 的 patch 文件启用模型
- [ ] patch 内容示例:
  ```yaml
  patch:
    grammar/language_model: wanxiang-lts-zh-hans
  ```

### 4. 新增: 皮肤 Patch
在 Go 版 theme patch (weasel/squirrel) 基础上，扩展对方案皮肤的 patch:
- [ ] 支持 Fcitx5 内置主题安装/设置 (同 Go 版)
- [ ] 支持 Rime 方案皮肤 patch (写入 `<engine>.custom.yaml`)
- [ ] 预置主题同 Go 版 (简纯、Win11、Mac、微信、鹿鸣、灵梦等)
- [ ] 自动检测平台并提供对应选项

### 5. 新增: Init 模式 (首次初始化)
`rime-init` 不仅是更新工具，还是初始化工具:
- [ ] 检测 Rime 引擎是否安装
- [ ] 引导用户选择方案 (薄雾/白霜/万象)
- [ ] 选择辅助码 (如有)
- [ ] 自动下载方案+词库+模型
- [ ] 自动 patch 配置 (模型启用 + 皮肤)
- [ ] 一键部署

## 平台特定路径

### Windows
- 配置: `%APPDATA%\rime-init\config.json`
- Rime 用户目录: `%APPDATA%\Rime\`
- 引擎: 小狼毫 (Weasel)

### macOS
- 配置: `~/Library/Application Support/rime-init/config.json`
- Rime 用户目录: `~/Library/Rime/`
- 引擎: 鼠须管 (Squirrel)

### Linux
- 配置: `~/.config/rime-init/config.json`
- Rime 用户目录: `~/.local/share/fcitx5/rime/` 或 `~/.config/ibus/rime/`
- 引擎: Fcitx5 / IBus
- Fcitx 兼容: `~/.config/fcitx/rime/`

## TUI 菜单结构

```
主菜单:
1. 一键更新 (检查并更新所有组件)
2. 更新方案
3. 更新词库
4. 更新模型
5. 模型 Patch (启用/禁用语言模型)
6. 自定义 (主题/皮肤)
7. 配置
8. 关于

配置菜单:
1. 选择方案
2. 选择辅助码
3. 下载源 (GitHub / CNB 镜像)
4. 代理设置
5. 自动更新
6. 排除文件
7. Fcitx 兼容 (Linux)
8. 语言

自定义菜单:
1. 切换程序主题
2. Rime 主题 Patch (Windows/macOS)
3. Fcitx5 主题 (Linux)
4. 活动主题设置
```

## 实现优先级

1. **Phase 1**: 基础框架 — config, types, api, fileutil
2. **Phase 2**: 核心更新器 — scheme/dict/model updater (先只支持万象)
3. **Phase 3**: TUI 框架 — 主菜单 + 更新进度
4. **Phase 4**: 部署器 — 跨平台 deploy
5. **Phase 5**: 扩展方案 — 薄雾/白霜支持
6. **Phase 6**: 模型 Patch
7. **Phase 7**: 皮肤/主题 Patch
8. **Phase 8**: Init 向导 + 完善

## 代码规范

- 使用 `anyhow::Result` 做错误处理，附加上下文
- 所有网络请求加超时 (默认 30s，下载不限)
- 下载使用流式写入 + 进度回调
- SHA256 校验在下载后自动执行
- 配置变更立即写盘
- TUI 使用 ratatui + crossterm，异步更新
- 平台特定代码用 `#[cfg(target_os = "...")]` 而非运行时判断
- 中文 UI 文本使用 &str 引用，避免重复分配
