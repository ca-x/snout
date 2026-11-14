<p align="center">
  <img src="assets/logo.png" alt="snout Logo" width="200" />
</p>

# snout

> **snout** /snaʊt/ — 象鼻虫的长吻。如同象鼻虫用细长的吻精准触达目标，snout 帮你精准触达 Rime 输入法的每个组件：方案、词库、模型、皮肤。
>
> The snout of a weevil — long, precise, reaching exactly where it needs to go. Just like that, snout reaches into every component of your Rime setup.

Rime 输入法初始化与更新工具，Rust 重写的 [rime-wanxiang-updater](https://github.com/ca-x/rime-wanxiang-updater)，支持 **万象**、**雾凇**、**白霜**、**薄荷**、**极点五笔86** 和 **智能 ABC·改**。

## 特性

- 🔄 **一键更新**: 方案、词库、模型一键检查并更新
- 🎨 **TUI 界面**: ratatui 终端界面，键盘操作
- 🖥️ **原生桌面端**: NSH 构建的 Windows / macOS / Linux 工作台，与 CLI/TUI 共用 Rust 核心
- 🌐 **多方案**: 万象 (10 变体) + 雾凇 + 白霜 + 薄荷 + 极点五笔86 + 智能 ABC·改
- 🧠 **模型 Patch**: 自动下载万象语法模型，并可按方案写入对应 schema patch
- 🎭 **皮肤 Patch**: 内置 6 个主题一键切换
- 🌍 **中英双语**: `--lang en` / `--lang zh`
- 🤖 **脚本友好**：非交互 CLI、JSON 输出、zsh/fish 自动补全
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

Linux 上运行 `cargo test` 需安装 `dbus-daemon`。Fcitx5 集成测试使用独立的临时 D-Bus，
不会读写当前桌面的输入法配置。

### 原生桌面端

桌面端覆盖命令行功能，并提供方案选择、组件更新、皮肤与候选数、网络与代理设置、
排除规则、诊断和 JSON / shell 补全导出。它直接调用共享 Rust 服务，不会通过拼接 shell 命令驱动 CLI。
耗时操作在后台工作线程中执行；更新可取消，覆盖用户数据和执行 hooks 前需显式确认。
“更新全部”和“检查更新”位于概览，组件更新紧邻对应状态。方案页支持“保存并安装”，也可仅保存；
普通设置直接保存，外观选好后点击“应用”。操作记录保留已完成的阶段，JSON 可展开查看；Shell 补全在设置的工具区导出。
CLI 与桌面端使用同一份配置；避免同时在多个窗口或终端中执行更新。
使用 hooks 的平台还需要在 `PATH` 中找到 `sh`；填写脚本文件路径，而不是命令字符串。

```bash
# Linux: GTK 4.14+，例如 Ubuntu 24.04
sudo apt-get install build-essential pkg-config libgtk-4-dev dbus-daemon

# Windows / macOS 无需安装 GTK；分别使用 Win32 / AppKit
cargo run --manifest-path desktop/Cargo.toml --release
```

构建结果位于 `desktop/target/release/snout-desktop`（Windows 为 `.exe`）。
桌面依赖独立于根 crate；仅构建 CLI 不会引入 GTK / AppKit / Win32 UI 依赖。
Release 中的桌面资产单独命名为 `snout-desktop-*`：Linux 为 GNU 动态链接包，
包含 `bin/` 和 `share/applications/`；macOS 为 `.app` 压缩包；Windows 为 ZIP。
Linux 运行时需要 GTK 4.14+。桌面包目前未签名或 notarize；打开前请核对发布页的 `SHA256SUMS`，
并使用操作系统的正常授权流程。`snout-bin` AUR 包仍安装 CLI/TUI，不包含桌面端。

### Arch Linux (AUR)

```bash
# AUR binary package (x86_64 / aarch64)
yay -S snout-bin

# or
paru -S snout-bin
```

桌面端使用独立的 `snout-desktop-bin` 包（目前仅 x86_64，需要 GTK 4.14+），
安装后可从应用菜单或通过 `snout-desktop` 启动。它可与 `snout-bin` 同时安装：

```bash
yay -S snout-desktop-bin
```

## 使用

### 组件状态与更新进度

桌面概览和 TUI 主菜单明确标注各组件「已是最新 / 需要更新 / 未安装 / 检查失败」，无需自己对比版本。
桌面会后台检查组件版本；已是最新的组件禁用更新按钮。离线、尚未检查和不适用的组件不会误报为最新。
安装与更新以「检查 → 下载 → 校验 → 安装与应用」时间线展示真实步骤，活动步骤带 loading 动画，跳过、失败和取消分别保留状态。
桌面原始日志收纳在「操作记录 → 诊断日志」，程序自更新也使用同一套步骤反馈。

```bash
snout status        # 只读检查方案、词库与模型的新旧状态，不安装或修改配置
snout status --json # 结构化组件状态；检查失败返回非零退出码
```

### 程序自更新

```bash
snout self-update --check        # 仅检查 snout 程序新版（默认行为）
snout self-update --yes          # 明确确认后下载、校验并替换程序
snout self-update --check --json # 机器可读结果
```

TUI 启动时后台检查，按 `U` 打开「程序更新」，查看后再次确认才会安装；安装完成后退出并重新运行。
桌面便携版在「设置 → 程序更新」确认后安装并重启。更新验证官方来源、SemVer、大小和 SHA-256，不会改动 Rime 数据。
**AUR 的 `snout-bin` / `snout-desktop-bin` 及其他系统包管理安装禁止程序自更新，即使以 root 运行也不例外。** 请通过 AUR 助手或包管理器升级。
`--update` 和输入方案、词库、模型自动更新仍只更新输入法数据，与程序自更新相互独立。
自行下载的终端程序请保留官方文件名 `snout` / `snout.exe`；开发构建不进行程序自更新。

### TUI 模式 (默认)

```bash
snout
```

TUI 主题默认为 `auto`。在支持 OSC 11 的终端中，它会读取终端背景色选择浅色或暗色配色；不支持时继续参考 `COLORFGBG`，最后回退到暗色。可在 TUI 设置中手动选择主题，或用 `SNOUT_TUI_THEME=light` / `dark` 临时覆盖。

启动交互式终端界面，使用 `↑↓/jk` 导航，`Enter` 确认，`q/Esc` 退出。

### 首次初始化

```bash
snout --init
```

引导选择方案、词库，自动下载并部署。

Linux 使用 Fcitx5 时，首次通过 `snout --init` 或 TUI 一键更新成功安装后，
会询问是否启用“自动检查并添加 Rime”，默认关闭。选择启用后立即添加，
并在后续一键更新（`snout --update` 或 TUI）结束后检查；关闭时跳过此步骤，减少等待。
可以随时在 TUI 设置中更改。已有配置未设置该选项时也默认关闭，在下一次交互式安装成功后询问。
自动添加会保留当前输入法组中已有的输入法、顺序和键盘布局，重复运行不会重复添加。
添加后可通过 Fcitx5 输入法快捷键切换到 Rime，更新不会主动切换当前输入法。
需先安装 `fcitx5-rime` 并在当前桌面会话中启动 Fcitx5；未满足条件时会显示原因和重试方法。
此步骤不安装软件包，也不设置桌面输入法框架或自启动。

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

### JSON 输出

非交互更新命令可追加 `--json`，stdout 只输出一个 JSON 文档，进度和诊断不会混入其中。
任一组件失败时 `ok` 为 `false`，进程退出码为非零，适合 shell、CI 和配置管理工具调用。

```bash
snout --update --json
snout --scheme --json | jq '.results'
```

```json
{
  "schema_version": 1,
  "ok": true,
  "command": "scheme",
  "schema": "WanxiangBase",
  "results": [
    {
      "component": "scheme",
      "display_name": "方案",
      "old_version": "2026.09.01",
      "new_version": "2026.09.22",
      "success": true,
      "message": "更新完成"
    }
  ],
  "error": null
}
```

`schema_version` 用于识别输出契约，`component` 是稳定的机器标识，`display_name` 和
`message` 会随 `--lang` 改变。参数解析失败，以及尚未产生组件结果的运行时失败，也使用
同一顶层结构并写入 `error`。组件执行失败则写入对应的 `results[].message`，此时顶层
`error` 为 `null`。

`--json` 不适用于交互式 `--init` 或默认 TUI。

### Shell 自动补全

```bash
# zsh
mkdir -p ~/.zfunc
snout completions zsh > ~/.zfunc/_snout
# 将 fpath=(~/.zfunc $fpath) 与 compinit 配置加入 ~/.zshrc

# fish
mkdir -p ~/.config/fish/completions
snout completions fish > ~/.config/fish/completions/snout.fish
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
| 万象拼音 (标准版) | [amzxyz/rime_wanxiang](https://github.com/amzxyz/rime_wanxiang) | 全拼、双拼；`--schema wanxiang` |
| 万象拼音 Pro | 同上 | 双拼 + 辅助码；分支参数见下表 |
| 雾凇拼音 | [iDvel/rime-ice](https://github.com/iDvel/rime-ice) | `--schema ice` |
| 白霜拼音 | [gaboolic/rime-frost](https://github.com/gaboolic/rime-frost) | `--schema frost` |
| 薄荷输入法 | [Mintimate/oh-my-rime](https://github.com/Mintimate/oh-my-rime) | `--schema mint` |
| 极点五笔86 | [KyleBing/rime-wubi86-jidian](https://github.com/KyleBing/rime-wubi86-jidian) | 五笔86；`--schema jidian` |
| 智能 ABC·改 | [cloudwu/abcm](https://github.com/cloudwu/abcm) | 智能 ABC 改版；`--schema abcm` |

### 拼音方案安装

桌面端选择方案，点击“保存并安装”；CLI 例如：

```bash
snout --schema ice --scheme
snout --schema mint --scheme
snout --schema abcm --scheme
```

万象 Pro 分支使用以下 `--schema` 参数：

| 辅助码 | 参数 | 辅助码 | 参数 |
|--------|------|--------|------|
| 墨奇 | `moqi` | 小鹤 | `flypy` |
| 自然码 | `zrm` | 虎码 | `tiger` |
| 五笔 | `wubi` | 汉心 | `hanxin` |
| 首右 | `shouyou` | 首右+ | `shyplus` |
| 万象 | `wx` | | |

例如 `snout --schema flypy --scheme`。更新已安装方案使用 `snout --update`。

### 极点五笔86与智能 ABC·改

桌面端选择方案，点击“保存并安装”；CLI 使用：

```bash
snout --schema jidian --scheme
snout --schema abcm --scheme
```

`jidian`（别名 `wubi86-jidian`）使用 `KyleBing/rime-wubi86-jidian`；
`abcm`（别名 `abc`）使用 `cloudwu/abcm`。两者按上游 `master` 提交检查更新，
词库随方案安装，不提供独立词库更新或万象模型补丁。ABC 会在缺少本地词典时补齐
`rime/rime-luna-pinyin` 的词典，需要 Rime Lua 插件。已有个性化 `.custom.yaml` 和平台皮肤不会被上游模板覆盖；新方案会加入切换菜单。

### Linux Fcitx5 在线主题

桌面端分为“本地主题”和“主题市场”：

1. 在“主题市场”下载仓库，完成后显示下载的主题数量。
2. 点击“查看主题”查看该仓库内的全部主题；一个仓库可以包含多个主题。
3. 在“本地主题”分别选择浅色、暗色模式的主题，点击“应用”。下载不会切换当前主题。

本地列表以“内置 / 在线 / 已安装”标签标记来源。市场从 GitHub 的
`fcitx5-theme`、`fcitx5-skin` topic 获取仓库，跨 topic 去重。

CLI 同样支持下载仓库内的多个主题：

```bash
# 列出内置、已安装和去重后的在线仓库；支持 --json
snout --list-fcitx5-themes --json

# 将指定 topic 仓库中的可用主题导入本地
snout --fetch-fcitx5-theme thep0y/fcitx5-themes-candlelight --json

# 使用列表或获取结果中的主题 key 应用亮暗组合
snout --fcitx5-theme-light <light-key> --fcitx5-theme-dark <dark-key>
```

在线查询使用共享 GitHub 认证（配置 Token、`GH_TOKEN`、`GITHUB_TOKEN`、可选的本机 `gh`）。
仅导入包含 `theme.conf` 的可直接使用主题，不执行仓库安装脚本；需先生成的仓库会提示参阅 README。
内置主题保持离线可用；TUI 的主题选择器也会列出已获取的在线主题。
圆角需主题支持，仅为支持切换的主题显示开关。
桌面端按实际引擎隐藏不适用的皮肤选项，Linux 不显示鼠须管 / 小狼毫补丁设置，其他平台不显示 Fcitx5 设置。

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
║  snout v0.2.5  万象拼音 (标准版)  ║
╚══════════════════════════════════════╝

  1. 一键更新
  2. 更新方案
  3. 更新词库
  4. 更新模型
  5. 模型 Patch
  6. 皮肤 Patch
  7. 切换方案
  8. 配置
  U. 程序更新
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
  "rime_auto_setup": null,
  "engine_sync_enabled": false,
  "engine_sync_use_link": true,
  "model_patch_enabled": false,
  "skin_patch_key": ""
}
```

`rime_auto_setup` 为 `true` 时自动检查并添加 Rime，`false` 时跳过；`null` 表示尚未选择，询问前也不会执行。

### GitHub 认证与 403 / 429

桌面端、CLI 和 TUI 使用相同的认证顺序：

1. 桌面设置 / 配置文件中的 `github_token`，或 CLI 的 `--github-token` 显式参数。
2. 环境变量 `GH_TOKEN`，其次 `GITHUB_TOKEN`。
3. 已安装且登录的 GitHub CLI：从 `gh auth token --hostname github.com` 读取现有凭据。
4. 均未提供时匿名访问。`gh` 不是必需依赖，程序不会自动安装它或启动交互式登录。

**没有 gh**：在桌面「设置 → 下载与网络 → GitHub 令牌」填写 GitHub Token 并保存；
CLI 可使用环境变量。例如在 Bash 中输入 Token（输入不回显），再执行更新：

```bash
read -rsp 'GitHub Token: ' GH_TOKEN
export GH_TOKEN
printf '\n'
snout --schema ice --update --json
```

已安装 gh 的用户也可先执行 `gh auth login --hostname github.com`。
环境变量和 gh 凭据只在内存中使用，不会自动写回配置文件，也不会发送给 CNB 镜像。
从应用菜单启动的桌面程序未必继承终端的环境变量；这种情况可直接在桌面设置填写 Token，
或从已设置环境变量的终端启动 `snout-desktop`。

401 会提示检查或更新 Token；403 不一定是限流，也可能与权限、访问策略或代理有关。
程序会根据 GitHub 响应头区分额度耗尽，显示恢复时间（UTC）和 `Retry-After` 等待时间。
401 / 403 / 429 不再立即重复请求；若已认证的额度也耗尽，仍需等待恢复。

## 架构

```
src/
├── main.rs           精简 CLI 入口
├── lib.rs            所有前端共用的 Rust crate
├── cli.rs            clap 参数解析、输出和自动补全
├── application/      状态检查
├── application.rs    更新调度、设置校验、模型 patch 一致性、外观能力
├── report.rs         CLI / GUI 共用的结构化结果
├── types.rs          核心类型 (Schema, Config, UpdateInfo)
├── config.rs         配置管理 + 平台路径检测
├── i18n.rs           国际化 (中/英)
├── api/mod.rs        GitHub / CNB API 客户端
├── fileutil/         SHA256, ZIP 解压
├── updater/          方案/词库/模型更新器 + model patch
├── deployer/         跨平台部署 + Fcitx 同步 + hooks
├── skin/             内置主题 + YAML patch
└── ui/               ratatui TUI + 初始化向导

desktop/              独立 NSH crate：原生 UI + 后台控制器
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

# 共享核心验证
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
cargo doc --no-deps

# 桌面 UI / 控制器无显示器交互测试
cargo test --manifest-path desktop/Cargo.toml --no-default-features --locked

# 原生桌面构建
cargo build --manifest-path desktop/Cargo.toml --release --locked
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
- 🖥️ **Native desktop**: An NSH workbench for Windows, macOS, and Linux using the same Rust application services
- 🌐 **Multi-schema**: Wanxiang (10 variants) + Rime Ice + Rime Frost + Mint Input
- 🧠 **Model patch**: Auto-download Wanxiang grammar models and write schema-specific patch files
- 🎭 **Skin patch**: 6 built-in themes, one-click apply
- 🌍 **i18n**: Chinese and English (`--lang en`)
- 🤖 **Automation-friendly**: Non-interactive CLI, JSON output, and zsh/fish completions
- 🪞 **CNB mirror**: Faster downloads in China
- 🔐 **SHA256 verification**: File integrity guaranteed
- 💾 **Cache reuse**: Skip re-downloading unchanged files
- 🔌 **Proxy**: SOCKS5 / HTTP support
- ⚡ **Cross-platform**: Windows / macOS / Linux

On Linux with Fcitx5, the first successful setup (`snout --init` or a TUI full update) asks
whether to enable automatic Rime checks and setup, defaulting to disabled. Enabling adds
Rime immediately and checks again after subsequent full updates (`snout --update` or the TUI).
Disabling skips this step to avoid waiting. Change the preference in TUI Settings at any time.
Existing configurations without a choice also default to disabled and prompt after the next
successful interactive setup. Automatic setup preserves existing input methods, their order
and keyboard layouts in the current group. Repeated runs do not add duplicates or
explicitly switch the active input method. Use your Fcitx5 shortcut to switch to Rime.
Install `fcitx5-rime` and start Fcitx5 in your desktop session first; setup reports missing
prerequisites with retry instructions. It does not install packages or configure the desktop
input framework or autostart.

## Install

```bash
git clone https://github.com/ca-x/snout.git
cd snout
cargo build --release
```

The optional native desktop is a separate crate:

```bash
# Linux needs GTK 4.14+, pkg-config and a C toolchain; macOS/Windows use native backends.
cargo run --manifest-path desktop/Cargo.toml --release
```

Desktop packages are separate `snout-desktop-*` release assets. Linux packages use GNU
dynamic linking and require GTK 4.14+; macOS archives contain an unsigned `.app`; Windows
archives contain the native executable. Verify `SHA256SUMS` before opening unsigned builds.
The desktop overview and TUI menu explicitly distinguish up-to-date, update available,
not installed, unchecked, failed and unsupported components. Installation progress uses
an event-driven step timeline with loading feedback; desktop diagnostic logs are collapsed.
Use `snout status` or `snout status --json` for read-only component checks without installing anything.

The `snout-bin` AUR package continues to install only CLI/TUI. Both frontends share the
existing configuration, so avoid running updates concurrently across them. The desktop
exposes all CLI capabilities, background progress/cancellation, explicit data/hook confirmation,
diagnostics, and JSON/completion export without shelling out to the CLI.
Hooks take script file paths and require `sh` on `PATH`; they are not command-string fields.

On Arch Linux, install the desktop with `yay -S snout-desktop-bin` (x86_64 only,
GTK 4.14+). It installs `snout-desktop` and an application-menu entry, and can coexist
with the CLI/TUI `snout-bin` package. Future releases update both AUR packages automatically.

## Usage

### Program self-updates

`snout self-update` or `snout self-update --check` checks only; add `--yes` to confirm
downloading, verifying and replacing the executable. `--json` is supported on this
subcommand. The TUI checks in the background and provides a separate `U` update page
with explicit confirmation and cancellable downloads. After installing, exit and
run snout again. Portable desktop builds confirm updates in Settings and restart.

**Both AUR packages (`snout-bin`, `snout-desktop-bin`) disable program self-updates.**
Use your AUR helper/package manager instead. System package paths are blocked even
when writable or running as root. Input-method updates (`--update`, schemes,
dictionaries and models) are separate and retain their original behavior.
Standalone terminal binaries must retain their official `snout` / `snout.exe` name;
development builds do not self-update. Stable releases publish a shared update
manifest covering all five terminal binaries and four desktop packages.

```bash
snout                  # Launch TUI
snout --init           # First-time setup wizard
snout --update         # Update everything
snout --update --mirror  # Use CNB mirror (China)
snout --scheme --mirror  # Update current scheme with mirror support (including Mint)
snout --update --proxy socks5://127.0.0.1:1080  # With proxy
snout --lang en --update  # English interface
```

The TUI theme defaults to `auto`. It reads the terminal background through OSC 11 when supported, then falls back to `COLORFGBG` and finally the dark palette. Choose a theme in TUI Settings or set `SNOUT_TUI_THEME=light` / `dark` to override it temporarily.

### JSON output

Append `--json` to a non-interactive update command. stdout contains exactly one JSON
document; progress and diagnostics stay out of the machine-readable stream. If any
component fails, `ok` is `false` and the process exits non-zero.

```bash
snout --update --json
snout --scheme --json | jq '.results'
```

`schema_version` identifies the output contract. `component` is a stable machine ID;
`display_name` and `message` follow `--lang`. Parse failures and runtime failures that
occur before a component result use the same top-level envelope and put their diagnostic
in `error`. Component failures use `results[].message`, with top-level `error` set to null.

`--json` is not available for the interactive `--init` wizard or the default TUI.

### Shell completions

```bash
# zsh
mkdir -p ~/.zfunc
snout completions zsh > ~/.zfunc/_snout
# Add fpath=(~/.zfunc $fpath) and compinit setup to ~/.zshrc

# fish
mkdir -p ~/.config/fish/completions
snout completions fish > ~/.config/fish/completions/snout.fish
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
