# BetterWX-GPUI

微信增强工具 [BetterWX](https://github.com/zetaloop/BetterWX) 的图形界面。GUI 用 [gpui-kit](https://github.com/longbridge/gpui-kit) 编写，**所有补丁逻辑仍由官方原版 `betterwx.exe` 执行**——本程序只把勾选状态翻译成原版的非交互 CLI 命令，不修改任何微信文件，也不改动原项目源码。

## 功能

- 18 项功能开关，自动读取真实状态（已启用 / 未启用 / 不可用）
- 系统通知头像、时间格式两种模式切换
- 批量应用更改、刷新状态、更新补丁、还原原版、`--force`
- 左侧功能列表 + 右侧实时日志（自动滚动到最新）
- 白框未选 / 绿底白勾的清晰勾选框
- 单文件 exe，管理员权限运行，不弹终端窗口

## 与原版的关系（重要）

**exe 不内嵌原版核心。** 程序启动时只从自身所在目录读取 `betterwx.exe`：

```
[exe 所在目录]/
├── BetterWX-GPUI.exe   ← 本 GUI
└── betterwx.exe        ← 官方原版核心（必须）
```

- 找不到 `betterwx.exe` 时，界面会显示红色提示条和下载按钮，所有功能锁定，不会执行任何操作。
- 升级原版：直接下载新版 `betterwx.exe` 覆盖同名文件即可，**不用重新编译 GUI**。
- 调用方式：`Command::new(betterwx.exe).args(...)`，走原版 CLI（`status` / `enable <id> [--mode m]` / `disable` / `update` / `restore`），并带 `CREATE_NO_WINDOW` 隐藏控制台。

修改补丁前请**完全退出微信（含托盘图标）**；微信升级后补丁会失效，用「更新补丁」重打。

## 构建

环境：Windows + Rust 稳定版（MSVC target）+ Windows SDK（提供 `mt.exe`）+ PowerShell。

一键构建（下载原版核心 → 编译 → 写管理员权限清单 → 打包 zip）：

```powershell
# 自动下载原版最新 release，zip 用当天日期作版本号
powershell -ExecutionPolicy Bypass -File scripts/build.ps1

# 指定版本号和核心版本
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -Version 1.0.0 -CoreVersion v2.1.1

# 离线构建（用现有 GPUI_KIT/vendor/betterwx.exe）
powershell -ExecutionPolicy Bypass -File scripts/build.ps1 -SkipDownload
```

也可双击 `GPUI_KIT/build_release.bat`（等价于无参数调用）。

产物：`GPUI_KIT/dist/BetterWX-GPUI-v<版本>-win-x64.zip`，内含 `BetterWX-GPUI.exe` + `betterwx.exe`，解压后保持同目录即可。

脚本用 `--jobs 1` 单线程编译：本机 Cargo 并行构建会卡死，别改。

## 目录结构

```
.
├── GPUI_KIT/
│   ├── src/main.rs          # 全部 UI 与 CLI 调用逻辑
│   ├── Cargo.toml
│   ├── admin.manifest       # requireAdministrator 权限清单
│   ├── build_release.bat    # 本地一键构建的 bat 包装
│   ├── vendor/              # 构建时下载的原版核心（gitignore）
│   └── dist/                # 构建产物（gitignore）
├── scripts/build.ps1              # 主构建脚本（本地 + CI 共用）
├── .github/workflows/             # GitHub Actions 工作流
└── .gitignore
```
