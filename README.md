# snout

> **snout** /snaʊt/ — 象鼻虫的长吻。如同象鼻虫用细长的吻精准触达目标，snout 帮你精准触达 Rime 输入法的每个组件：方案、词库、模型、皮肤。
>
> The snout of a weevil — long, precise, reaching exactly where it needs to go. Just like that, snout reaches into every component of your Rime setup.

Rime 输入法初始化与更新工具，Rust 重写的 [rime-wanxiang-updater](https://github.com/ca-x/rime-wanxiang-updater)，支持 **万象**、**雾凇**、**白霜** 三大方案。

## 特性

- 🔄 **一键更新**: 方案、词库、模型一键检查并更新
- 🎨 **TUI 界面**: ratatui 终端界面，键盘操作
- 🌐 **多方案**: 万象 (10 变体) + 雾凇 + 白霜
- 🧠 **模型 Patch**: 自动下载并启用万象语法模型
- 🎭 **皮肤 Patch**: 内置 6 个主题一键切换
- 🌍 **中英双语**: `--lang en` / `--lang zh`
- 🪞 **CNB 镜像**: 国内加速下载
- 🔐 **SHA256 校验**: 确保文件完整性
- 💾 **断点续传**: 节省流量
- 🔌 **代理支持**: SOCKS5 / HTTP
- ⚡ **跨平台**: Windows / macOS / Linux

## 安装

### 从源码编译

```bash
git clone https://github.com/ca-x/snout.git
cd snout
cargo build --release
# 二进制在 target/release/snout
```

### Arch Linux (AUR)

```bash
# 待发布
yay -S snout
```

## 使用

### TUI 模式 (默认)

```bash
snout
```

启动交互式终端界面，使用 `↑↓/jk` 导航，`Enter` 确认，`q/Esc` 退出。

### 首次初始化

```bash
snout --init
```

引导选择方案、词库，自动下载并部署。

### 命令行模式

```bash
# 一键更新所有
snout --update

# 仅更新方案
snout --scheme

# 仅更新词库
snout --dict

# 仅更新模型
snout --model

# 更新模型并启用 patch
snout --model --patch-model
```

### 其他选项

```bash
# 使用 CNB 镜像 (国内加速)
snout --update --mirror

# 设置代理
snout --update --proxy socks5://127.0.0.1:1080

# 英文界面
snout --lang en --update
```

## 支持的方案

| 方案 | 仓库 | 说明 |
|------|------|------|
| 万象拼音 (标准版) | [amzxyz/rime_wanxiang](https://github.com/amzxyz/rime_wanxiang) | 全拼、双拼 |
| 万象拼音 Pro (墨奇/小鹤/自然码/虎码/五笔/汉心/首右) | 同上 | 双拼 + 辅助码 |
| 雾凇拼音 | [iDvel/rime-ice](https://github.com/iDvel/rime-ice) | 16.6k ⭐ |
| 白霜拼音 | [gaboolic/rime-frost](https://github.com/gaboolic/rime-frost) | 3.1k ⭐ |

### 语法模型 (仅万象)

从 [amzxyz/RIME-LMDG](https://github.com/amzxyz/RIME-LMDG) 下载 `wanxiang-lts-zh-hans.gram`，自动 patch 到方案配置。

## TUI 菜单

```
╔══════════════════════════════════════╗
║  snout v0.1.0  万象拼音 (标准版)  ║
╚══════════════════════════════════════╝

  1. 一键更新
  2. 更新方案
  3. 更新词库
  4. 更新模型
  5. 模型 Patch
  6. 皮肤 Patch
  7. 切换方案
  8. 配置
  Q. 退出
```

## 内置皮肤

- 简纯 (amzxyz)
- Win11 浅色 / 暗色
- 微信
- Mac 白
- 灵梦

皮肤写入 `weasel.custom.yaml` (Windows) 或 `squirrel.custom.yaml` (macOS)。

## 配置

配置文件位置:

- **Linux**: `~/.config/snout/config.json`
- **macOS**: `~/Library/Application Support/snout/config.json`
- **Windows**: `%APPDATA%\snout\config.json`

```json
{
  "schema": "WanxiangBase",
  "use_mirror": false,
  "github_token": "",
  "proxy_enabled": false,
  "proxy_type": "socks5",
  "proxy_address": "127.0.0.1:1080",
  "exclude_files": [".DS_Store", ".git"],
  "auto_update": false,
  "language": "zh",
  "fcitx_compat": false,
  "model_patch_enabled": false,
  "skin_patch_key": ""
}
```

## 架构

```
src/
├── main.rs           CLI 入口
├── types.rs          核心类型 (Schema, Config, UpdateInfo)
├── config.rs         配置管理 + 平台路径检测
├── i18n.rs           国际化 (中/英)
├── api/mod.rs        GitHub / CNB API 客户端
├── fileutil/         SHA256, ZIP 解压
├── updater/          方案/词库/模型更新器 + model patch
├── deployer/         跨平台部署 + Fcitx 同步 + hooks
├── skin/             内置主题 + YAML patch
└── ui/               ratatui TUI + 初始化向导
```

## 开发

```bash
# 开发构建
cargo build

# Release 构建
cargo build --release

# 运行
cargo run
cargo run -- --init
cargo run -- --update --mirror
```

## 贡献

欢迎提交 Issue 和 PR！

## 许可证

MIT

## 致谢

- [rime-wanxiang-updater](https://github.com/ca-x/rime-wanxiang-updater) - Go 原版
- [rime_wanxiang](https://github.com/amzxyz/rime_wanxiang) - 万象拼音方案
- [rime-ice](https://github.com/iDvel/rime-ice) - 雾凇拼音方案
- [rime-frost](https://github.com/gaboolic/rime-frost) - 白霜拼音方案
- [RIME-LMDG](https://github.com/amzxyz/RIME-LMDG) - 语法模型
- [ratatui](https://github.com/ratatui/ratatui) - TUI 框架
- [reqwest](https://github.com/seanmonstar/reqwest) - HTTP 客户端

---

# English

> **snout** /snaʊt/ — the elongated rostrum of a weevil. Like a weevil reaching precisely into a seed, snout reaches into every component of your Rime input method: schemas, dictionaries, models, and skins.

A Rime input method initialization & update tool. Rust rewrite of [rime-wanxiang-updater](https://github.com/ca-x/rime-wanxiang-updater), supporting **Wanxiang**, **Rime Ice**, and **Rime Frost** schemas.

## Features

- 🔄 **One-click update**: Schemas, dictionaries, and language models in one command
- 🎨 **TUI**: Interactive terminal UI powered by ratatui
- 🌐 **Multi-schema**: Wanxiang (10 variants) + Rime Ice + Rime Frost
- 🧠 **Model patch**: Auto-download and enable Wanxiang grammar model
- 🎭 **Skin patch**: 6 built-in themes, one-click apply
- 🌍 **i18n**: Chinese and English (`--lang en`)
- 🪞 **CNB mirror**: Faster downloads in China
- 🔐 **SHA256 verification**: File integrity guaranteed
- 💾 **Cache reuse**: Skip re-downloading unchanged files
- 🔌 **Proxy**: SOCKS5 / HTTP support
- ⚡ **Cross-platform**: Windows / macOS / Linux

## Install

```bash
git clone https://github.com/ca-x/snout.git
cd snout
cargo build --release
```

## Usage

```bash
snout                  # Launch TUI
snout --init           # First-time setup wizard
snout --update         # Update everything
snout --update --mirror  # Use CNB mirror (China)
snout --update --proxy socks5://127.0.0.1:1080  # With proxy
snout --lang en --update  # English interface
```

## License

MIT
