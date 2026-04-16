# snout

> **snout** /snaʊt/ — 象鼻虫的长吻。如同象鼻虫用细长的吻精准触达目标，snout 帮你精准触达 Rime 输入法的每个组件：方案、词库、模型、皮肤。
>
> The snout of a weevil — long, precise, reaching exactly where it needs to go. Just like that, snout reaches into every component of your Rime setup.

Rime 输入法初始化与更新工具，Rust 重写的 [rime-wanxiang-updater](https://github.com/ca-x/rime-wanxiang-updater)，支持 **万象**、**雾凇**、**白霜**、**薄荷** 四大方案。

## 特性

- 🔄 **一键更新**: 方案、词库、模型一键检查并更新
- 🎨 **TUI 界面**: ratatui 终端界面，键盘操作
- 🌐 **多方案**: 万象 (10 变体) + 雾凇 + 白霜 + 薄荷
- 🧠 **模型 Patch**: 自动下载万象语法模型，并可按方案写入对应 schema patch
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
# AUR binary package (x86_64 / aarch64)
yay -S snout-bin

# or
paru -S snout-bin
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

# 首次初始化时可直接选择薄荷方案
snout --init

# 或者先在配置 / TUI 中把当前方案切到 Mint，再执行：
snout --scheme
```

### 其他选项

```bash
# 使用 CNB 镜像 (国内加速)
snout --update --mirror

# 薄荷方案同样支持镜像下载
snout --scheme --mirror

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
| 薄荷输入法 | [Mintimate/oh-my-rime](https://github.com/Mintimate/oh-my-rime) | 薄荷系配置模板与词库 |

### 功能支持矩阵

| 能力 | 万象 | 雾凇 | 白霜 | 薄荷 |
|------|------|------|------|------|
| 方案更新 | ✅ | ✅ | ✅ | ✅ |
| 独立词库更新 | ✅ | ✅ | ❌ | ❌ |
| 万象模型下载 | ✅ | ✅ | ✅ | ✅ |
| 模型 patch 目标 | `wanxiang*.custom.yaml` | `rime_ice.custom.yaml` | `rime_frost.custom.yaml` | `rime_mint.custom.yaml` |
| 皮肤 patch | Windows / macOS | Windows / macOS | Windows / macOS | Windows / macOS |

### 语法模型 / 模型 Patch

从 [amzxyz/RIME-LMDG](https://github.com/amzxyz/RIME-LMDG) 下载 `wanxiang-lts-zh-hans.gram`。

当前支持的模型 patch 目标：

- **万象**：写入对应 `wanxiang*.custom.yaml`
- **雾凇**：写入 `rime_ice.custom.yaml`
- **白霜**：写入 `rime_frost.custom.yaml`
- **薄荷**：写入 `rime_mint.custom.yaml`

模型 patch 采用当前上游 schema 的 `grammar/language` 键写入模型名；雾凇 / 白霜 / 薄荷还会一并写入 `translator/contextual_suggestions`、`translator/max_homophones`、`translator/max_homographs` 等配套参数。皮肤 patch 仍只写到输入法应用配置文件（`weasel.custom.yaml` / `squirrel.custom.yaml`）。

### 薄荷方案说明

薄荷方案来源于 [Mintimate/oh-my-rime](https://github.com/Mintimate/oh-my-rime) 仓库。

`snout` 在部署薄荷时会只保留 Rime 运行所需资源，例如：

- `rime_mint*.yaml`
- `dicts/`
- `lua/`
- `opencc/`
- `weasel.yaml`
- `squirrel.yaml`

不会把仓库里的 README、CI、Issue 模板等非运行时文件复制到用户 Rime 目录。

## TUI 菜单

```
╔══════════════════════════════════════╗
║  snout v0.1.5  万象拼音 (标准版)  ║
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
  "engine_sync_enabled": false,
  "engine_sync_use_link": true,
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
- [oh-my-rime](https://github.com/Mintimate/oh-my-rime) - 薄荷输入法方案
- [RIME-LMDG](https://github.com/amzxyz/RIME-LMDG) - 语法模型
- [ratatui](https://github.com/ratatui/ratatui) - TUI 框架
- [reqwest](https://github.com/seanmonstar/reqwest) - HTTP 客户端

---

# English

> **snout** /snaʊt/ — the elongated rostrum of a weevil. Like a weevil reaching precisely into a seed, snout reaches into every component of your Rime input method: schemas, dictionaries, models, and skins.

A Rime input method initialization & update tool. Rust rewrite of [rime-wanxiang-updater](https://github.com/ca-x/rime-wanxiang-updater), supporting **Wanxiang**, **Rime Ice**, **Rime Frost**, and **Mint Input** schemas.

## Features

- 🔄 **One-click update**: Schemas, dictionaries, and language models in one command
- 🎨 **TUI**: Interactive terminal UI powered by ratatui
- 🌐 **Multi-schema**: Wanxiang (10 variants) + Rime Ice + Rime Frost + Mint Input
- 🧠 **Model patch**: Auto-download Wanxiang grammar models and write schema-specific patch files
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
snout --scheme --mirror  # Update current scheme with mirror support (including Mint)
snout --update --proxy socks5://127.0.0.1:1080  # With proxy
snout --lang en --update  # English interface
```

To initialize Mint specifically, choose **Mint Input** in the setup wizard, then
run either:

```bash
snout --init
```

or update the current scheme directly after switching the saved config to Mint:

```bash
snout --scheme
snout --scheme --mirror
```

## Supported Schemes

- Wanxiang
- Rime Ice
- Rime Frost
- Mint Input

### Capability Matrix

| Capability | Wanxiang | Rime Ice | Rime Frost | Mint Input |
|-----------|----------|----------|------------|------------|
| Scheme update | ✅ | ✅ | ✅ | ✅ |
| Independent dictionary update | ✅ | ✅ | ❌ | ❌ |
| Wanxiang model download | ✅ | ✅ | ✅ | ✅ |
| Model patch target | `wanxiang*.custom.yaml` | `rime_ice.custom.yaml` | `rime_frost.custom.yaml` | `rime_mint.custom.yaml` |
| Skin patch | Windows / macOS | Windows / macOS | Windows / macOS | Windows / macOS |

## Model Patch

`snout` downloads `wanxiang-lts-zh-hans.gram` from `amzxyz/RIME-LMDG` and can
write scheme-specific custom patch files for:

- `wanxiang*.custom.yaml`
- `rime_ice.custom.yaml`
- `rime_frost.custom.yaml`
- `rime_mint.custom.yaml`

Ice, Frost, and Mint use schema-level `patch:` overrides for grammar and
translator parameters, and Wanxiang also uses `grammar/language` to match the
current upstream schema. Skin patching remains limited to application-specific
config files such as `weasel.custom.yaml` and `squirrel.custom.yaml`.

## Mint Scheme Notes

Mint support is sourced from
[Mintimate/oh-my-rime](https://github.com/Mintimate/oh-my-rime).

When deploying Mint, `snout` keeps runtime Rime assets only, such as:

- `rime_mint*.yaml`
- `dicts/`
- `lua/`
- `opencc/`
- `weasel.yaml`
- `squirrel.yaml`

Repository metadata like README files, CI config, and issue templates are not
copied into the user Rime directory.

## License

MIT
MIT
