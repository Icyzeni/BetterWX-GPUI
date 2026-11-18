//! BetterWX 图形控制台 — GPUI Kit 版本
//!
//! 与 Python 版本一样，这里只做界面：所有补丁分析 / 写入 / 还原都委托给
//! 原版 betterwx.exe 的非交互 CLI。本 crate 不修改任何微信文件。
//!
//! 原项目: https://github.com/zetaloop/BetterWX (v2.1.1)
//! UI 套件: https://github.com/longbridge/gpui-kit (0.9.1)

// 使用 Windows GUI 子系统，双击运行时不会弹出控制台窗口。
#![cfg_attr(all(not(debug_assertions), target_os = "windows"), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme, Disableable, Theme, ThemeMode};
use gpui_kit::base::{StyledExt as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Bounds, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Point, Render, ScrollHandle, SharedString, Size, StatefulInteractiveElement as _, Styled as _,
    Window, WindowBounds, WindowOptions, div, px, rgb,
};

// ---------------------------------------------------------------------------
// 原版功能表（与 `betterwx --help` 一致）
// ---------------------------------------------------------------------------
struct FeatureDef {
    id: &'static str,
    name: &'static str,
    desc: &'static str,
    modes: &'static [(&'static str, &'static str)],
}

static FEATURES: &[FeatureDef] = &[
    FeatureDef { id: "login", name: "自动登录", desc: "启动时自动点击登录按钮；重复启动唤起已有窗口，防止顶号", modes: &[] },
    FeatureDef { id: "keep-login", name: "保留登录信息", desc: "登录失败时不清空登录信息，除非它真的失效", modes: &[] },
    FeatureDef { id: "revoke", name: "防撤回", desc: "拦截消息撤回，自己发出的消息仍可正常撤回", modes: &[] },
    FeatureDef { id: "notify", name: "系统通知", desc: "把托盘消息改为系统通知", modes: &[("square", "方形头像"), ("circle", "圆形头像")] },
    FeatureDef { id: "time-format", name: "时间格式", desc: "精确到秒，或始终显示完整日期时间", modes: &[("seconds", "精确到秒"), ("full", "完整时间")] },
    FeatureDef { id: "timestamps", name: "逐条显示时间", desc: "每条消息都显示时间戳", modes: &[] },
    FeatureDef { id: "emoticon", name: "表情复制为图片", desc: "预览和复制动画表情", modes: &[] },
    FeatureDef { id: "writable", name: "不要只读保存", desc: "保存文件时不要附加只读属性", modes: &[] },
    FeatureDef { id: "resources", name: "防止文件过期", desc: "更积极地自动下载和保留聊天资源", modes: &[] },
    FeatureDef { id: "browser", name: "使用系统浏览器", desc: "用系统默认浏览器打开网页", modes: &[] },
    FeatureDef { id: "no-appex", name: "关闭内置浏览器", desc: "禁用 WeChatAppEx；小程序与内置网页将不可用", modes: &[] },
    FeatureDef { id: "uia", name: "无障碍读取控件", desc: "允许屏幕阅读器读取微信控件", modes: &[] },
    FeatureDef { id: "remote", name: "兼容外部工具", desc: "缓解模拟输入、无障碍、Agent、远控的兼容性问题", modes: &[] },
    FeatureDef { id: "no-updates", name: "不要更新", desc: "阻止微信自动下载安装更新", modes: &[] },
    FeatureDef { id: "coexist", name: "多账号共存", desc: "不同字母入口记住各自账号，共用程序与聊天数据", modes: &[] },
    FeatureDef { id: "sound", name: "自定义提示音", desc: "替换微信内置的消息提示音", modes: &[] },
    FeatureDef { id: "tray-icon", name: "自定义托盘图标", desc: "替换微信托盘图标", modes: &[] },
    FeatureDef { id: "tray-flash", name: "关闭托盘闪烁", desc: "新消息到达时不闪烁托盘图标", modes: &[] },
];

#[derive(Clone, Copy)]
enum LogKind {
    Cmd,
    Out,
    Ok,
    Err,
    Info,
}

#[derive(Clone, Default)]
struct FeatureState {
    wanted: bool,
    current: bool,
    available: bool,
    status: String,
    mode: usize,
    current_mode: usize,
}

struct WorkResult {
    logs: Vec<(String, LogKind)>,
    status: Option<String>,
}

#[derive(Clone, Copy)]
struct Colors {
    background: Hsla,
    surface: Hsla,
    border: Hsla,
    foreground: Hsla,
    muted: Hsla,
    primary: Hsla,
    success: Hsla,
    danger: Hsla,
    radius: gpui_kit::Pixels,
}

pub struct Console {
    rows: Vec<FeatureState>,
    log: Vec<(String, LogKind)>,
    log_scroll: ScrollHandle,
    busy: bool,
    detected: String,
    force: bool,
    core: PathBuf,
    started: bool,
    themed: bool,
    missing_core: bool,
}

// ---------------------------------------------------------------------------
// 与原版 CLI 的交互
// ---------------------------------------------------------------------------
/// 官方原版下载地址（GitHub Release）。UI 只负责提示，不自动下载。
pub const CORE_DOWNLOAD_URL: &str =
    "https://github.com/zetaloop/BetterWX/releases/latest";

/// 只从 exe 同目录读取原版 betterwx.exe —— 与原版分离，方便单独升级。
fn find_core() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    exe_dir.join("betterwx.exe")
}

fn clean_output(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            while let Some(&next) = chars.peek() {
                chars.next();
                if next.is_ascii_alphabetic() || next == '~' {
                    break;
                }
            }
        } else if ch != '\r' {
            out.push(ch);
        }
    }
    out
}

/// 用系统默认浏览器打开下载页（不弹控制台窗口）。
fn open_download_page() {
    #[cfg(windows)]
    {
        let mut command = Command::new("rundll32.exe");
        command
            .args(["url.dll,FileProtocolHandler", CORE_DOWNLOAD_URL])
            .stdin(Stdio::null())
            .creation_flags(0x0800_0000);
        let _ = command.spawn();
    }
}

fn run_one(core: &Path, args: &[String]) -> (bool, String) {
    let mut command = Command::new(core);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW — GUI 程序不要弹控制台窗口
        command.creation_flags(0x0800_0000);
    }
    match command.output() {
        Ok(output) => {
            let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&output.stderr));
            (output.status.success(), clean_output(&text))
        }
        Err(error) => (false, format!("无法启动核心程序：{error}")),
    }
}

fn execute_all(core: &Path, commands: &[Vec<String>]) -> WorkResult {
    let mut logs = Vec::new();
    let mut status = None;
    let total = commands.len();
    for (index, args) in commands.iter().enumerate() {
        let prefix = if total > 1 {
            format!("[{}/{}] ", index + 1, total)
        } else {
            String::new()
        };
        logs.push((
            format!("> {}betterwx {}", prefix, args.join(" ")),
            LogKind::Cmd,
        ));
        let (success, text) = run_one(core, args);
        for line in text.lines() {
            if !line.trim().is_empty() {
                logs.push((line.to_owned(), LogKind::Out));
            }
        }
        if !success {
            logs.push(("命令执行失败，后续操作已中止".into(), LogKind::Err));
            break;
        }
        if args.first().is_some_and(|value| value == "status") {
            status = Some(text);
        }
    }
    WorkResult { logs, status }
}

// ---------------------------------------------------------------------------
// 界面状态
// ---------------------------------------------------------------------------
impl Console {
    pub fn new() -> Self {
        let core = find_core();
        let missing_core = !core.is_file();
        let mut log = vec![
            ("BetterWX 图形控制台 · GPUI Kit 0.9.1".into(), LogKind::Info),
            (format!("核心程序：{}", core.display()), LogKind::Info),
            ("提示：修改补丁前请先完全退出微信（含托盘图标）。".into(), LogKind::Info),
        ];
        if missing_core {
            log.push((
                "!! 未找到原版 betterwx.exe，所有功能已锁定。".into(),
                LogKind::Err,
            ));
            log.push((
                format!("请把官方 betterwx.exe 放到本 exe 同目录：{}", core.parent().map(|p| p.display().to_string()).unwrap_or_default()),
                LogKind::Err,
            ));
            log.push((
                format!("下载地址：{}", CORE_DOWNLOAD_URL),
                LogKind::Err,
            ));
            log.push((
                "下载后选择 betterwx.exe 资产，解压与本程序放在同一文件夹。".into(),
                LogKind::Err,
            ));
        }
        Self {
            rows: vec![FeatureState::default(); FEATURES.len()],
            log,
            log_scroll: ScrollHandle::new(),
            busy: false,
            detected: "检测目标：—".into(),
            force: false,
            core,
            started: false,
            themed: false,
            missing_core,
        }
    }

    fn tail(&self) -> Vec<String> {
        if self.force {
            vec!["--force".into()]
        } else {
            Vec::new()
        }
    }

    fn pending(&self) -> Vec<Vec<String>> {
        let mut commands = Vec::new();
        for (index, feature) in FEATURES.iter().enumerate() {
            let row = &self.rows[index];
            if !row.available {
                continue;
            }
            let mode_changed = !feature.modes.is_empty() && row.mode != row.current_mode;
            if row.wanted && (!row.current || mode_changed) {
                let mut args = vec!["enable".into(), feature.id.into()];
                if !feature.modes.is_empty() {
                    args.push("--mode".into());
                    args.push(feature.modes[row.mode].0.into());
                }
                args.extend(self.tail());
                commands.push(args);
            } else if !row.wanted && row.current {
                let mut args = vec!["disable".into(), feature.id.into()];
                args.extend(self.tail());
                commands.push(args);
            }
        }
        commands
    }

    fn start_work(&mut self, mut commands: Vec<Vec<String>>, cx: &mut Context<Self>) {
        if self.busy || self.missing_core {
            return;
        }
        // 每轮工作都以刷新状态收尾，界面永远反映原版的真实结果。
        commands.push(vec!["status".into()]);
        self.busy = true;
        self.log.push(("正在执行…".into(), LogKind::Info));
        cx.notify();

        let core = self.core.clone();
        let task = cx
            .background_executor()
            .spawn(async move { execute_all(&core, &commands) });
        let view = cx.entity();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            view.update(cx, |this, cx| this.finish(result, cx));
        })
        .detach();
    }

    fn finish(&mut self, result: WorkResult, cx: &mut Context<Self>) {
        for entry in result.logs {
            self.log.push(entry);
        }
        if let Some(status) = result.status {
            self.apply_status(&status);
        }
        self.busy = false;
        self.log.push(("状态已刷新".into(), LogKind::Ok));
        if self.log.len() > 600 {
            let drain = self.log.len() - 600;
            self.log.drain(..drain);
        }
        // 可选排查日志：设置 BETTERWX_GPUI_LOG=<路径> 后把界面日志同步写盘。
        if let Ok(path) = std::env::var("BETTERWX_GPUI_LOG") {
            let text = self
                .log
                .iter()
                .map(|(line, kind)| {
                    let prefix = match kind {
                        LogKind::Cmd => "$ ",
                        LogKind::Err => "! ",
                        LogKind::Ok => "+ ",
                        LogKind::Info => "# ",
                        LogKind::Out => "  ",
                    };
                    format!("{prefix}{line}")
                })
                .collect::<Vec<_>>()
                .join("\n");
            let _ = std::fs::write(path, text);
        }
        // 日志始终显示最新一行
        self.log_scroll.scroll_to_bottom();
        cx.notify();
    }

    fn apply_status(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.trim();
            if let Some(target) = line.strip_prefix("目标：") {
                self.detected = format!("检测目标：{}", target.trim());
                continue;
            }
            let Some((name, state)) = line.split_once('：') else {
                continue;
            };
            let Some(index) = FEATURES.iter().position(|f| f.name == name.trim()) else {
                continue;
            };
            let state = state.trim().to_owned();
            let unavailable = state.starts_with("不可用") || state == "未知";
            let enabled = !unavailable && state.split('，').next() != Some("未启用");
            let clean = state.split('，').next().unwrap_or("");
            let mode = FEATURES[index]
                .modes
                .iter()
                .position(|(_, label)| *label == clean)
                .unwrap_or(0);
            let row = &mut self.rows[index];
            row.available = !unavailable;
            row.current = enabled;
            row.current_mode = mode;
            row.mode = mode;
            row.wanted = enabled;
            row.status = state;
        }
    }

    fn colors(&self, cx: &Context<Self>) -> Colors {
        let theme = cx.theme();
        Colors {
            background: theme.background,
            surface: theme.secondary,
            border: theme.border,
            foreground: theme.foreground,
            muted: theme.muted_foreground,
            primary: theme.primary,
            success: theme.success,
            danger: theme.danger,
            radius: theme.radius,
        }
    }

    fn log_color(kind: LogKind, colors: Colors) -> Hsla {
        match kind {
            LogKind::Cmd => colors.primary,
            LogKind::Ok => colors.success,
            LogKind::Err => colors.danger,
            LogKind::Info => colors.muted,
            LogKind::Out => colors.foreground,
        }
    }
}

impl Render for Console {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.themed {
            self.themed = true;
            Theme::change(ThemeMode::Dark, Some(window), cx);
            window.set_window_title("BetterWX · GPUI Kit");
        }
        if !self.started {
            self.started = true;
            self.start_work(Vec::new(), cx);
        }

        let colors = self.colors(cx);
        let pending_count = self.pending().len();
        let busy = self.busy;
        let force = self.force;
        let detected = self.detected.clone();

        // 自定义勾选框配色：未选中=白框，选中=绿底白勾
        let check_green: Hsla = rgb(0x22C55E).into();
        let check_white: Hsla = rgb(0xFFFFFF).into();
        let check_empty: Hsla = rgb(0x141414).into();

        // ---- 顶栏（固定高度 + 两端对齐，杜绝 flex 挤压） ----------------
        let header = h_flex()
            .flex_none()
            .h(px(46.))
            .items_center()
            .justify_between()
            .px_3()
            .border_b_1()
            .border_color(colors.border)
            .child(
                div()
                    .flex_none()
                    .text_sm()
                    .font_medium()
                    .whitespace_nowrap()
                    .child("BetterWX"),
            )
            .child(
                h_flex()
                    .flex_none()
                    .items_center()
                    .gap_2()
                    .child(
                        Checkbox::new(900usize)
                            .label("强制")
                            .checked(force)
                            .on_change(cx.listener(|this, value, _, cx| {
                                this.force = *value;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("refresh")
                            .label("刷新")
                            .outline()
                            .compact()
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| this.start_work(Vec::new(), cx))),
                    )
                    .child(
                        Button::new("update")
                            .label("更新补丁")
                            .outline()
                            .compact()
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let mut args = vec!["update".into()];
                                args.extend(this.tail());
                                this.start_work(vec![args], cx);
                            })),
                    )
                    .child(
                        Button::new("restore")
                            .label("还原")
                            .danger()
                            .compact()
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let mut args = vec!["restore".into()];
                                args.extend(this.tail());
                                this.start_work(vec![args], cx);
                            })),
                    )
                    .child(
                        Button::new("apply")
                            .label(format!(
                                "应用{}",
                                if pending_count > 0 {
                                    format!(" ({pending_count})")
                                } else {
                                    String::new()
                                }
                            ))
                            .primary()
                            .compact()
                            .disabled(busy || pending_count == 0)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let commands = this.pending();
                                if !commands.is_empty() {
                                    this.start_work(commands, cx);
                                }
                            })),
                    ),
            );

        // ---- 目标信息 ---------------------------------------------------
        let target_bar = h_flex()
            .flex_none()
            .h(px(28.))
            .items_center()
            .justify_between()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(colors.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_xs()
                    .text_color(colors.muted)
                    .child(SharedString::from(detected)),
            )
            .child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_xs()
                    .text_color(colors.muted)
                    .child(if busy {
                        "● 正在执行原版命令，请勿关闭"
                    } else {
                        "就绪 · 勾选后点「应用更改」"
                    }),
            );

        // ---- 功能列表 ---------------------------------------------------
        let mut list = v_flex()
            .id("features")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_2()
            .gap_1();

        for index in 0..FEATURES.len() {
            let feature = &FEATURES[index];
            let row = &self.rows[index];
            let badge_color = if !row.available {
                colors.danger
            } else if row.current {
                colors.success
            } else {
                colors.muted
            };
            let badge_text = if row.status.is_empty() {
                "读取中…".to_owned()
            } else if row.status.starts_with("不可用") {
                "不可用".to_owned()
            } else {
                row.status.split('，').next().unwrap_or("").to_owned()
            };

            let mut info = v_flex()
                .flex_1()
                .gap_1()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(div().font_medium().child(feature.name))
                        .child(
                            div()
                                .text_xs()
                                .px_2()
                                .rounded(px(9.))
                                .bg(colors.surface)
                                .text_color(badge_color)
                                .child(SharedString::from(badge_text)),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(colors.muted)
                        .child(feature.desc),
                );

            if !feature.modes.is_empty() {
                info = info.child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_xs().text_color(colors.muted).child("模式"))
                        .child(
                            Button::new(index * 2 + 1)
                                .label(format!("{} ⇄", feature.modes[row.mode].1))
                                .outline()
                                .compact()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let count = FEATURES[index].modes.len();
                                    this.rows[index].mode = (this.rows[index].mode + 1) % count;
                                    cx.notify();
                                })),
                        ),
                );
            }

            let usable = row.available && !busy;
            let check_bg = if row.wanted { check_green } else { check_empty };
            let check_border = if row.wanted { check_green } else { check_white };

            let card = h_flex()
                .flex_none()
                .w_full()
                .items_start()
                .gap_2()
                .p_2()
                .rounded(colors.radius)
                .border_1()
                .border_color(colors.border)
                .bg(colors.surface)
                .child(
                    div()
                        .id(index * 2)
                        .flex_none()
                        .size(px(18.))
                        .mt(px(1.))
                        .rounded(px(4.))
                        .border_1()
                        .border_color(check_border)
                        .bg(check_bg)
                        .opacity(if usable { 1. } else { 0.4 })
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(if row.wanted {
                            div()
                                .text_xs()
                                .font_medium()
                                .text_color(check_white)
                                .child("✓")
                        } else {
                            div()
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if !this.rows[index].available || this.busy {
                                return;
                            }
                            this.rows[index].wanted = !this.rows[index].wanted;
                            cx.notify();
                        })),
                )
                .child(info);
            list = list.child(card);
        }

        // ---- 日志面板（右侧约 1/3 宽） ---------------------------------
        let log_handle = self.log_scroll.clone();
        let mut log_panel = v_flex()
            .id("log")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&log_handle)
            .p_2()
            .gap(px(2.));
        log_panel = log_panel.children(self.log.iter().map(|(text, kind)| {
            div()
                .text_xs()
                .text_color(Self::log_color(*kind, colors))
                .child(SharedString::from(text.as_str()))
        }));

        let log_side = v_flex()
            .flex_none()
            .w(px(232.))
            .h_full()
            .border_l_1()
            .border_color(colors.border)
            .bg(colors.background)
            .child(
                h_flex()
                    .flex_none()
                    .px_2()
                    .py_1()
                    .gap_2()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .child("运行日志"),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(colors.muted)
                            .child(format!("{} 条", self.log.len())),
                    ),
            )
            .child(log_panel);

        // ---- 左右分栏：左侧功能列表（约 2/3），右侧日志（约 1/3） -------
        let body = div()
            .flex()
            .flex_row()
            .flex_1()
            .min_h_0()
            .child(list.flex_none().w(px(488.)))
            .child(log_side);

        // ---- 缺失核心时的醒目下载提示 ---------------------------------
        let missing_core = self.missing_core;
        let warning_bar = h_flex()
            .flex_none()
            .w_full()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .bg(colors.danger.opacity(0.15))
            .border_b_1()
            .border_color(colors.danger)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_medium()
                            .text_color(colors.danger)
                            .whitespace_nowrap()
                            .child("未找到原版 betterwx.exe"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(colors.foreground)
                            .child("请下载官方原版，并把 betterwx.exe 放到本 exe 同一目录后重启本程序。"),
                    ),
            )
            .child(
                Button::new("open-download")
                    .label("打开下载页")
                    .primary()
                    .compact()
                    .on_click(cx.listener(|_, _, _, _| open_download_page())),
            );

        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(header)
            .child(target_bar);
        if missing_core {
            root = root.child(warning_bar);
        }
        root.child(body)
    }
}

fn main() {
    gpui_kit::application().run(move |cx| {
        // 必须在使用任何 gpui-kit 组件前调用
        gpui_kit::init(cx);

        let mut options = WindowOptions::default();
        options.window_bounds = Some(WindowBounds::Windowed(Bounds {
            origin: Point {
                x: px(120.),
                y: px(80.),
            },
            size: Size {
                width: px(720.),
                height: px(640.),
            },
        }));
        options.is_resizable = true;

        gpui_kit::open_window(options, cx, |_, cx| {
            cx.new(|_| Console::new())
        })
        .expect("failed to open window");
    });
}
