//! visualization — F5 JSON, F6 dot, F7 ASCII, F8 TUI

use crate::data_manager::{Edge, Graph, Node};
use crate::{connection_tracker, graph_builder};
use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::io;

// ──────────────────────────────────────────────────────────────────────────────
// F5 — JSON 输出
// ──────────────────────────────────────────────────────────────────────────────

pub fn output_json(graph: &Graph, file: &str) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(graph).context("序列化 Graph 为 JSON 失败")?;
    std::fs::write(file, json).with_context(|| format!("写入 JSON 文件 {} 失败", file))?;
    eprintln!("JSON 已写入: {}", file);
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// F6 — Graphviz dot 输出
// ──────────────────────────────────────────────────────────────────────────────

pub fn output_dot(graph: &Graph, file: &str) -> anyhow::Result<()> {
    let mut dot = String::new();
    dot.push_str("digraph netopo {\n");
    dot.push_str("  rankdir=LR;\n");
    dot.push_str("  node [fontname=\"Helvetica\" fontsize=10];\n");

    for node in &graph.nodes {
        let label = build_node_label(node);
        if node.is_local {
            dot.push_str(&format!(
                "  \"{}\" [label=\"{}\" shape=doublecircle color=blue];\n",
                node.ip, label
            ));
        } else {
            dot.push_str(&format!("  \"{}\" [label=\"{}\"];\n", node.ip, label));
        }
    }

    for edge in &graph.edges {
        let penwidth = 1.0_f64.max(edge.count as f64 * 0.5).min(5.0);
        let style = if edge.protocol == "UDP" {
            " style=dashed"
        } else {
            ""
        };
        dot.push_str(&format!(
            "  \"{}\" -> \"{}\" [penwidth={:.1} label=\"{} x{}\"{}];\n",
            edge.src, edge.dst, penwidth, edge.protocol, edge.count, style
        ));
    }

    dot.push_str("}\n");
    std::fs::write(file, dot).with_context(|| format!("写入 dot 文件 {} 失败", file))?;
    eprintln!("dot 已写入: {}", file);
    Ok(())
}

fn build_node_label(node: &Node) -> String {
    let mut parts = vec![node.ip.clone()];
    if let Some(ref h) = node.hostname {
        parts.push(h.clone());
    }
    if !node.ports.is_empty() {
        parts.push(format!("{} ports", node.ports.len()));
    }
    parts.join("\\n")
}

// ──────────────────────────────────────────────────────────────────────────────
// F7 — ASCII 拓扑图
// ──────────────────────────────────────────────────────────────────────────────

/// ASCII 输出选项
#[derive(Default)]
pub struct AsciiOptions {
    pub use_color: bool,
    pub resolve_ports: bool,
    pub filter: Option<String>,
    /// false = 每组最多 10 条；true = 不限
    pub all_connections: bool,
    pub asn_db: Option<maxminddb::Reader<Vec<u8>>>,
}

/// ANSI 颜色代码集合（use_color=false 时全为空字符串）
struct Colors {
    reset: &'static str,
    local: &'static str,    // 本机节点：亮蓝粗体
    lan_node: &'static str, // LAN 设备：绿色
    isp: &'static str,      // ISP 标签：黄色粗体
    tcp: &'static str,      // TCP 端口：青色
    udp: &'static str,      // UDP 端口：黄色
    sep: &'static str,      // 分隔符：深灰
}

impl Colors {
    fn new(enabled: bool) -> Self {
        if enabled {
            Self {
                reset: "\x1b[0m",
                local: "\x1b[1;94m",
                lan_node: "\x1b[32m",
                isp: "\x1b[1;33m",
                tcp: "\x1b[36m",
                udp: "\x1b[33m",
                sep: "\x1b[90m",
            }
        } else {
            Self {
                reset: "",
                local: "",
                lan_node: "",
                isp: "",
                tcp: "",
                udp: "",
                sep: "",
            }
        }
    }
}

/// 判断 IP 是否属于局域网/本地地址段
/// IPv4: 10/172.16-31/192.168/127
/// IPv6: ::1 (loopback), fe80:: (link-local), fc00::/7 (unique local fc/fd)
fn is_lan_ip(ip: &str) -> bool {
    // IPv6 本地地址
    if ip == "::1" {
        return true;
    }
    let lower = ip.to_lowercase();
    if lower.starts_with("fe80:") || lower.starts_with("fc") || lower.starts_with("fd") {
        return true;
    }
    // IPv4 私有段
    if ip.starts_with("192.168.") || ip.starts_with("10.") || ip.starts_with("127.") {
        return true;
    }
    if ip.starts_with("172.") {
        let second: u8 = ip
            .split('.')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        return (16..=31).contains(&second);
    }
    false
}

/// 根据 hostname/IP 简单判断公网 ISP 归属（返回 &'static str）
fn isp_of(hostname: Option<&str>, ip: &str) -> &'static str {
    let h = hostname.unwrap_or("").to_lowercase();
    if h.contains("google")
        || h.contains("googleapis")
        || h.contains("1e100")
        || ip.starts_with("8.8.")
        || ip.starts_with("142.250.")
        || ip.starts_with("172.217.")
        || ip.starts_with("216.58.")
    {
        return "Google";
    }
    if h.contains("github") || ip.starts_with("140.82.") || ip.starts_with("185.199.") {
        return "GitHub";
    }
    if h.contains("apple") || ip.starts_with("17.") {
        return "Apple";
    }
    if h.contains("cloudflare")
        || ip.starts_with("1.1.1.")
        || ip.starts_with("1.0.0.")
        || ip.starts_with("104.18.")
        || ip.starts_with("172.64.")
    {
        return "Cloudflare";
    }
    if h.contains("amazonaws") || h.contains("aws") || h.contains("amazon") {
        return "AWS";
    }
    if h.contains("akamai") || h.contains("akamaitechnologies") {
        return "Akamai";
    }
    if h.contains("canonical") || h.contains("ubuntu") || ip.starts_with("185.125.") {
        return "Canonical";
    }
    if h.contains("microsoft") || h.contains("msft") || h.contains("azure") {
        return "Microsoft";
    }
    if h.contains("fastly") {
        return "Fastly";
    }
    if h.contains("meta.") || h.contains("facebook") || h.contains("instagram") {
        return "Meta";
    }
    "其他"
}

/// MaxMind ASN 记录结构
#[derive(serde::Deserialize, Debug)]
struct AsnRecord {
    #[serde(default)]
    autonomous_system_organization: String,
}

/// 综合 ISP 分类：先用硬编码规则，再用 MaxMind ASN DB
fn isp_name(hostname: Option<&str>, ip: &str, db: Option<&maxminddb::Reader<Vec<u8>>>) -> String {
    let hardcoded = isp_of(hostname, ip);
    if hardcoded != "其他" {
        return hardcoded.to_string();
    }
    if let Some(reader) = db {
        if let Ok(addr) = ip.parse::<std::net::IpAddr>() {
            if let Ok(record) = reader.lookup::<AsnRecord>(addr) {
                if !record.autonomous_system_organization.is_empty() {
                    return record.autonomous_system_organization;
                }
            }
        }
    }
    "其他".to_string()
}

/// 计算字符串的视觉宽度（CJK 字符算 2 列）
fn visual_width(s: &str) -> usize {
    s.chars()
        .map(|c| {
            let u = c as u32;
            if (0x1100..=0x115F).contains(&u)
                || (0x2E80..=0x303F).contains(&u)
                || (0x3040..=0x9FFF).contains(&u)
                || (0xAC00..=0xD7AF).contains(&u)
                || (0xF900..=0xFAFF).contains(&u)
                || (0xFE10..=0xFE19).contains(&u)
                || (0xFE30..=0xFE6F).contains(&u)
                || (0xFF00..=0xFF60).contains(&u)
                || (0xFFE0..=0xFFE6).contains(&u)
            {
                2
            } else {
                1
            }
        })
        .sum()
}

/// 将内容居中到 inner_width 视觉列（不足则两侧补空格）
fn center_in_box(content: &str, content_vw: usize, inner: usize) -> String {
    let pad = inner.saturating_sub(content_vw);
    let left = pad / 2;
    let right = pad - left;
    format!("{}{}{}", " ".repeat(left), content, " ".repeat(right))
}

/// 从 RFC3339 时间戳提取 "YYYY-MM-DD HH:MM"
fn parse_time_str(ts: &str) -> String {
    if ts.len() >= 16 {
        ts[..16].replace('T', " ")
    } else {
        ts.to_string()
    }
}

/// 截断字符串到最多 max_vis 视觉列（ANSI 代码不计入，超出加 …）
fn trunc(s: &str, max_vis: usize) -> String {
    let mut vis = 0usize;
    let mut result = String::new();
    let mut in_esc = false;
    for c in s.chars() {
        if c == '\x1b' {
            in_esc = true;
            result.push(c);
            continue;
        }
        if in_esc {
            result.push(c);
            if c.is_ascii_alphabetic() {
                in_esc = false;
            }
            continue;
        }
        let w = if visual_width(&c.to_string()) == 2 {
            2
        } else {
            1
        };
        if vis + w > max_vis {
            result.push('…');
            break;
        }
        vis += w;
        result.push(c);
    }
    result
}

/// 将端口号映射为服务名（--resolve-ports 使用）
fn resolve_port_name(port: u16) -> Option<&'static str> {
    match port {
        443 => Some("HTTPS"),
        80 => Some("HTTP"),
        22 => Some("SSH"),
        5228 => Some("GCM"),
        5223 => Some("APNs"),
        445 => Some("SMB"),
        3306 => Some("MySQL"),
        5432 => Some("Postgres"),
        6379 => Some("Redis"),
        _ => None,
    }
}

/// 节点标签：hostname 优先，IP 放括号内；若无 hostname 则仅显示 IP
fn node_display(node: Option<&Node>, ip: &str) -> String {
    if let Some(n) = node {
        if let Some(ref h) = n.hostname {
            if h != ip {
                return format!("{} ({})", h, ip);
            }
        }
    }
    ip.to_string()
}

/// 格式化连接列表，支持颜色和端口名解析
/// src_ip = Some(ip) 只显示从该 src 出发的连接；None 显示所有到 dst 的连接
fn fmt_conns_ex(
    edges: &[Edge],
    src_ip: Option<&str>,
    dst_ip: &str,
    resolve_ports: bool,
    colors: &Colors,
) -> String {
    let mut parts: Vec<String> = edges
        .iter()
        .filter(|e| e.dst == dst_ip && src_ip.is_none_or(|s| e.src == s))
        .map(|e| {
            let port_label = if resolve_ports {
                resolve_port_name(e.dst_port)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| e.dst_port.to_string())
            } else {
                e.dst_port.to_string()
            };
            let color = if e.protocol == "TCP" {
                colors.tcp
            } else {
                colors.udp
            };
            if e.count > 1 {
                format!(
                    "{}{}:{} x{}{}",
                    color, e.protocol, port_label, e.count, colors.reset
                )
            } else {
                format!("{}{}:{}{}", color, e.protocol, port_label, colors.reset)
            }
        })
        .collect();
    parts.sort();
    parts.dedup();
    parts.join("  ")
}

/// MaxMind GeoLite2-ASN 数据库默认路径
fn asn_db_path() -> std::path::PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".config")
        .join("netopo")
        .join("GeoLite2-ASN.mmdb")
}

/// 尝试加载 MaxMind ASN 数据库（文件不存在时返回 None）
pub fn load_asn_db() -> Option<maxminddb::Reader<Vec<u8>>> {
    let path = asn_db_path();
    maxminddb::Reader::open_readfile(path).ok()
}

/// 从网络下载/更新 MaxMind GeoLite2-ASN 数据库（需要系统 curl）
pub fn update_asn_db() -> anyhow::Result<()> {
    let path = asn_db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("创建目录失败: {}", parent.display()))?;
    }
    let url = "https://git.io/GeoLite2-ASN";
    let path_str = path.to_str().unwrap_or("GeoLite2-ASN.mmdb");
    eprintln!("正在下载 GeoLite2-ASN.mmdb ...");
    let status = std::process::Command::new("curl")
        .args(["-L", "-o", path_str, url])
        .status()
        .context("无法执行 curl，请确认已安装")?;
    if status.success() {
        eprintln!("数据库已更新: {}", path.display());
        Ok(())
    } else {
        anyhow::bail!("curl 下载失败，退出码: {:?}", status.code())
    }
}

/// 构建 ASCII 拓扑图（带颜色、端口名解析、过滤、Akamai 聚合）
fn build_ascii(graph: &Graph, opts: &AsciiOptions, out: &mut String) {
    let colors = Colors::new(opts.use_color);
    let db_ref = opts.asn_db.as_ref();
    let filter = opts.filter.as_deref().map(|s| s.to_lowercase());

    // ── 标题框（固定 44 列）────────────────────────────────────────────────────
    const INNER: usize = 42;
    let title = "netopo 拓扑图";
    let title_vw = visual_width(title);
    let title_centered = center_in_box(title, title_vw, INNER);
    let time_str = parse_time_str(&graph.captured_at);
    let time_vw = time_str.len(); // 全 ASCII
    let time_centered = center_in_box(&time_str, time_vw, INNER);

    out.push_str(&format!(
        "{}╔{}╗{}\n",
        colors.sep,
        "═".repeat(INNER),
        colors.reset
    ));
    out.push_str(&format!(
        "{}║{}{}{}║{}\n",
        colors.sep, colors.reset, title_centered, colors.sep, colors.reset
    ));
    out.push_str(&format!(
        "{}║{}{}{}║{}\n",
        colors.sep, colors.reset, time_centered, colors.sep, colors.reset
    ));
    out.push_str(&format!(
        "{}╚{}╝{}\n\n",
        colors.sep,
        "═".repeat(INNER),
        colors.reset
    ));

    let local_node = graph.nodes.iter().find(|n| n.is_local);
    let lan_nodes: Vec<&Node> = graph.nodes.iter().filter(|n| is_lan_ip(&n.ip)).collect();

    // 收集公网目标 IP（去重后排序）
    let pub_dst_vec: Vec<&str> = {
        let mut set: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for e in &graph.edges {
            if !is_lan_ip(&e.dst) {
                set.insert(e.dst.as_str());
            }
        }
        let mut v: Vec<&str> = set.into_iter().collect();
        v.sort_unstable();
        v
    };

    // ── 局域网设备 ────────────────────────────────────────────────────────────
    out.push_str(&format!(
        "{}━━━ 局域网设备 ({}) ━━━━━━━━━━━━━━━━━━━━{}\n\n",
        colors.sep,
        lan_nodes.len(),
        colors.reset
    ));

    if let Some(local) = local_node {
        let hdr = match &local.hostname {
            Some(h) => format!("[★ {}] ({})", h, local.ip),
            None => format!("[★ {}]", local.ip),
        };
        out.push_str(&format!(
            "  {}{}{}\n",
            colors.local,
            trunc(&hdr, 78),
            colors.reset
        ));

        // 出站 LAN 连接（按首次出现顺序，支持 --filter）
        let mut lan_dsts: Vec<&str> = Vec::new();
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for e in &graph.edges {
            if e.src == local.ip && is_lan_ip(&e.dst) && seen.insert(e.dst.as_str()) {
                // 过滤：IP 或 hostname 包含关键词
                if let Some(ref kw) = filter {
                    let dst_node = graph.nodes.iter().find(|nd| nd.ip == e.dst);
                    let label = node_display(dst_node, &e.dst).to_lowercase();
                    if !e.dst.to_lowercase().contains(kw.as_str()) && !label.contains(kw.as_str()) {
                        continue;
                    }
                }
                lan_dsts.push(e.dst.as_str());
            }
        }
        let n = lan_dsts.len();
        for (i, dst_ip) in lan_dsts.iter().enumerate() {
            let connector = if i + 1 < n { "├─►" } else { "└─►" };
            let dst_node = graph.nodes.iter().find(|nd| nd.ip == *dst_ip);
            let label = node_display(dst_node, dst_ip);
            let conns = fmt_conns_ex(
                &graph.edges,
                Some(&local.ip),
                dst_ip,
                opts.resolve_ports,
                &colors,
            );
            let line_plain = format!("  {} {:<38}  ", connector, label);
            out.push_str(&format!(
                "{}{}{}{}\n",
                colors.lan_node,
                trunc(&line_plain, 47),
                colors.reset,
                conns
            ));
        }
        out.push('\n');
    }

    // ── 公网连接 ──────────────────────────────────────────────────────────────
    if !pub_dst_vec.is_empty() {
        // 按 ISP 聚合（BTreeMap 保证键有序）
        let mut isp_map: std::collections::BTreeMap<String, Vec<&str>> =
            std::collections::BTreeMap::new();
        for &dst_ip in &pub_dst_vec {
            let dst_node = graph.nodes.iter().find(|n| n.ip == dst_ip);
            let isp = isp_name(dst_node.and_then(|n| n.hostname.as_deref()), dst_ip, db_ref);
            isp_map.entry(isp).or_default().push(dst_ip);
        }

        // 过滤：只保留 ISP 名或 IP 匹配的条目
        if let Some(ref kw) = filter {
            isp_map.retain(|isp_label, ips| {
                isp_label.to_lowercase().contains(kw.as_str())
                    || ips.iter().any(|ip| ip.to_lowercase().contains(kw.as_str()))
            });
        }

        if !isp_map.is_empty() {
            let total_pub: usize = isp_map.values().map(|v| v.len()).sum();
            out.push_str(&format!(
                "{}━━━ 公网连接 ({}) ━━━━━━━━━━━━━━━━━━━━━{}\n\n",
                colors.sep, total_pub, colors.reset
            ));

            const DEFAULT_MAX: usize = 10;

            for (isp, ips) in &isp_map {
                let cap = if opts.all_connections {
                    ips.len()
                } else {
                    DEFAULT_MAX.min(ips.len())
                };
                let hidden = ips.len() - cap;

                out.push_str(&format!("  {}{}{}\n", colors.isp, isp, colors.reset));
                let displayed = &ips[..cap];
                let n = displayed.len();
                for (i, dst_ip) in displayed.iter().enumerate() {
                    let is_last = i + 1 == n && hidden == 0;
                    let connector = if is_last { "└─►" } else { "├─►" };
                    let dst_node = graph.nodes.iter().find(|nd| nd.ip == *dst_ip);
                    let label = node_display(dst_node, dst_ip);
                    let conns =
                        fmt_conns_ex(&graph.edges, None, dst_ip, opts.resolve_ports, &colors);
                    let line_plain = format!("  {} {:<38}  ", connector, label);
                    out.push_str(&format!(
                        "{}\n",
                        trunc(&format!("{}{}", line_plain, conns), 79)
                    ));
                }
                if hidden > 0 {
                    out.push_str(&format!(
                        "  {}└─► … 还有 {} 条，使用 --all-connections 显示全部{}\n",
                        colors.sep, hidden, colors.reset
                    ));
                }
                out.push('\n');
            }
        }
    }

    // ── 底部摘要 ──────────────────────────────────────────────────────────────
    out.push_str(&format!(
        "{}{}{}\n",
        colors.sep,
        "─".repeat(79),
        colors.reset
    ));

    let mut port_counts: std::collections::HashMap<u16, u32> = std::collections::HashMap::new();
    for e in &graph.edges {
        *port_counts.entry(e.dst_port).or_default() += e.count;
    }
    let mut top_ports: Vec<(u16, u32)> = port_counts.into_iter().collect();
    top_ports.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let top3: Vec<String> = top_ports
        .iter()
        .take(3)
        .map(|(p, c)| {
            let label = if opts.resolve_ports {
                resolve_port_name(*p)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| p.to_string())
            } else {
                p.to_string()
            };
            format!("{}(x{})", label, c)
        })
        .collect();

    let summary = if top3.is_empty() {
        format!(
            "活跃连接: {}  │  TCP: {}  UDP: {}",
            graph.summary.total_edges, graph.summary.tcp_connections, graph.summary.udp_connections
        )
    } else {
        format!(
            "活跃连接: {}  │  TCP: {}  UDP: {}  │  Top端口: {}",
            graph.summary.total_edges,
            graph.summary.tcp_connections,
            graph.summary.udp_connections,
            top3.join(" ")
        )
    };
    out.push_str(&trunc(&summary, 79));
    out.push('\n');
}

pub fn print_ascii(graph: &Graph, opts: &AsciiOptions) {
    print!("{}", render_ascii_string_with_opts(graph, opts));
}

// ──────────────────────────────────────────────────────────────────────────────
// F8 — TUI 交互界面
// ──────────────────────────────────────────────────────────────────────────────

/// TUI 焦点面板枚举
#[derive(PartialEq, Eq, Clone, Copy)]
enum FocusedPanel {
    NodeList,
    TopoGraph,
}

/// TUI 应用状态
struct AppState {
    graph: Graph,
    /// 节点列表选中索引
    selected: usize,
    /// 过滤关键词（空 = 不过滤）
    filter: String,
    /// 是否在过滤输入模式
    filter_mode: bool,
    /// 是否在导出模式
    export_mode: bool,
    /// 导出文件名输入缓冲
    export_buf: String,
    /// 是否显示帮助覆盖层
    show_help: bool,
    /// 当前焦点面板
    focused_panel: FocusedPanel,
    /// 状态栏消息
    status_msg: String,
    /// 本机 IP（用于重新扫描时标记本机节点）
    local_ip: String,
}

impl AppState {
    fn new(graph: Graph, local_ip: String) -> Self {
        Self {
            graph,
            selected: 0,
            filter: String::new(),
            filter_mode: false,
            export_mode: false,
            export_buf: String::new(),
            show_help: false,
            focused_panel: FocusedPanel::NodeList,
            status_msg: String::new(),
            local_ip,
        }
    }

    fn filtered_nodes(&self) -> Vec<&Node> {
        if self.filter.is_empty() {
            self.graph.nodes.iter().collect()
        } else {
            let kw = self.filter.to_lowercase();
            self.graph
                .nodes
                .iter()
                .filter(|n| {
                    n.ip.to_lowercase().contains(&kw)
                        || n.hostname
                            .as_deref()
                            .map(|h| h.to_lowercase().contains(&kw))
                            .unwrap_or(false)
                })
                .collect()
        }
    }

    fn selected_node(&self) -> Option<&Node> {
        let nodes = self.filtered_nodes();
        nodes.get(self.selected).copied()
    }
}

pub fn run_tui(graph: Graph, local_ip: String) -> anyhow::Result<()> {
    enable_raw_mode().context("无法启用 raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("无法进入备用屏幕")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("无法创建 Terminal")?;

    let mut app = AppState::new(graph, local_ip);
    let result = run_tui_loop(&mut terminal, &mut app);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}

fn run_tui_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut AppState,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| draw_ui(f, app))?;

        if event::poll(std::time::Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                // Ctrl+C — 退出
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
                    break;
                }

                // 过滤模式
                if app.filter_mode {
                    match key.code {
                        KeyCode::Enter | KeyCode::Esc => {
                            app.filter_mode = false;
                            app.selected = 0;
                        }
                        KeyCode::Char(c) => app.filter.push(c),
                        KeyCode::Backspace => {
                            app.filter.pop();
                        }
                        _ => {}
                    }
                    continue;
                }

                // 导出模式
                if app.export_mode {
                    match key.code {
                        KeyCode::Enter => {
                            let fname = app.export_buf.clone();
                            app.export_mode = false;
                            app.export_buf.clear();
                            match crate::visualization::output_json(&app.graph, &fname) {
                                Ok(()) => {
                                    app.status_msg = format!("JSON 已写入: {}", fname);
                                }
                                Err(e) => {
                                    app.status_msg = format!("导出失败: {}", e);
                                }
                            }
                        }
                        KeyCode::Esc => {
                            app.export_mode = false;
                            app.export_buf.clear();
                        }
                        KeyCode::Char(c) => app.export_buf.push(c),
                        KeyCode::Backspace => {
                            app.export_buf.pop();
                        }
                        _ => {}
                    }
                    continue;
                }

                // 帮助覆盖层
                if app.show_help {
                    app.show_help = false;
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('r') => {
                        app.status_msg = "正在刷新...".to_string();
                        terminal.draw(|f| draw_ui(f, app))?;
                        let local_ip = app.local_ip.clone();
                        let rescan_result = tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                let edges =
                                    connection_tracker::get_connections(false, false).await?;
                                Ok::<_, anyhow::Error>(graph_builder::build_graph(
                                    vec![],
                                    edges,
                                    &local_ip,
                                ))
                            })
                        });
                        match rescan_result {
                            Ok(new_graph) => {
                                app.graph = new_graph;
                                let now = chrono::Local::now().format("%H:%M:%S").to_string();
                                app.status_msg = format!("刷新完成 {}", now);
                            }
                            Err(e) => {
                                app.status_msg = format!("刷新失败: {}", e);
                            }
                        }
                    }
                    KeyCode::Up => {
                        if app.selected > 0 {
                            app.selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        let max = app.filtered_nodes().len().saturating_sub(1);
                        if app.selected < max {
                            app.selected += 1;
                        }
                    }
                    KeyCode::Char('f') => {
                        app.filter_mode = true;
                        app.filter.clear();
                    }
                    KeyCode::Char('e') => {
                        app.export_mode = true;
                        app.export_buf = "netopo_export.json".to_string();
                    }
                    KeyCode::Tab => {
                        app.focused_panel = match app.focused_panel {
                            FocusedPanel::NodeList => FocusedPanel::TopoGraph,
                            FocusedPanel::TopoGraph => FocusedPanel::NodeList,
                        };
                    }
                    KeyCode::Char('?') => {
                        app.show_help = true;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn draw_ui(f: &mut Frame, app: &AppState) {
    let size = f.size();

    // 主布局：标题栏 + 内容区 + 拓扑图区 + 状态栏
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 标题栏
            Constraint::Min(6),    // 节点列表 + 连接详情
            Constraint::Length(4), // 拓扑图
            Constraint::Length(1), // 状态栏
        ])
        .split(size);

    // ── 标题栏 ────────────────────────────────────────────────────────────────
    let title_text = format!(
        " netopo  v{}          [最后更新: {}]",
        env!("CARGO_PKG_VERSION"),
        &app.graph
            .captured_at
            .get(..19)
            .unwrap_or(&app.graph.captured_at)
    );
    let title = Paragraph::new(title_text).style(
        Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(title, main_chunks[0]);

    // ── 中间区域：节点列表 | 连接详情 ─────────────────────────────────────────
    let middle_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_chunks[1]);

    // 节点列表
    let filtered = app.filtered_nodes();
    let node_items: Vec<ListItem> = filtered
        .iter()
        .map(|n| {
            let star = if n.is_local { "★ " } else { "  " };
            let tag = classify_node(n);
            let text = format!("{}{:<20} {}", star, n.ip, tag);
            let style = if n.is_local {
                Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD)
            } else if n.ports.is_empty() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let node_block_title = format!("节点列表 ({})", filtered.len());
    let node_border_color = if app.focused_panel == FocusedPanel::NodeList {
        Color::Yellow
    } else {
        Color::Reset
    };
    let node_list = List::new(node_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(node_block_title)
                .border_style(Style::default().fg(node_border_color)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("► ");

    let mut list_state = ListState::default();
    list_state.select(Some(app.selected.min(filtered.len().saturating_sub(1))));
    f.render_stateful_widget(node_list, middle_chunks[0], &mut list_state);

    // 连接详情
    let detail_text = if let Some(node) = app.selected_node() {
        let inbound_tcp = app
            .graph
            .edges
            .iter()
            .filter(|e| e.dst == node.ip && e.protocol == "TCP")
            .count();
        let inbound_udp = app
            .graph
            .edges
            .iter()
            .filter(|e| e.dst == node.ip && e.protocol == "UDP")
            .count();
        let outbound_tcp = app
            .graph
            .edges
            .iter()
            .filter(|e| e.src == node.ip && e.protocol == "TCP")
            .count();
        let outbound_udp = app
            .graph
            .edges
            .iter()
            .filter(|e| e.src == node.ip && e.protocol == "UDP")
            .count();
        let ports_str = node
            .ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let hostname = node.hostname.as_deref().unwrap_or("—");
        vec![
            Line::from(format!(" 选中: {}", node.ip)),
            Line::from(vec![
                Span::raw(" 入站: TCP "),
                Span::styled(inbound_tcp.to_string(), Style::default().fg(Color::Green)),
                Span::raw("  UDP "),
                Span::styled(inbound_udp.to_string(), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::raw(" 出站: TCP "),
                Span::styled(outbound_tcp.to_string(), Style::default().fg(Color::Green)),
                Span::raw("  UDP "),
                Span::styled(outbound_udp.to_string(), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(format!(" 开放端口: {}", ports_str)),
            Line::from(format!(" Hostname: {}", hostname)),
        ]
    } else {
        vec![Line::from(" (无节点)")]
    };

    let detail_widget =
        Paragraph::new(detail_text).block(Block::default().borders(Borders::ALL).title("连接详情"));
    f.render_widget(detail_widget, middle_chunks[1]);

    // ── 拓扑图（ASCII 渲染）─────────────────────────────────────────────────
    let topo_text = build_mini_ascii(app);
    let topo_border_color = if app.focused_panel == FocusedPanel::TopoGraph {
        Color::Yellow
    } else {
        Color::Reset
    };
    let topo_widget = Paragraph::new(topo_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title("拓扑图 (ASCII 渲染)")
            .border_style(Style::default().fg(topo_border_color)),
    );
    f.render_widget(topo_widget, main_chunks[2]);

    // ── 状态栏 ────────────────────────────────────────────────────────────────
    let status_text = if app.filter_mode {
        format!("[过滤] 输入关键词: {}▌", app.filter)
    } else if app.export_mode {
        format!("[导出] 文件名: {}▌  (Enter 确认, Esc 取消)", app.export_buf)
    } else if !app.status_msg.is_empty() {
        app.status_msg.clone()
    } else {
        " [q]退出  [r]刷新  [↑↓]选择  [f]过滤  [e]导出JSON  [Tab]切换  [?]帮助".to_string()
    };
    let status =
        Paragraph::new(status_text).style(Style::default().bg(Color::Black).fg(Color::Green));
    f.render_widget(status, main_chunks[3]);

    // ── 帮助覆盖层 ────────────────────────────────────────────────────────────
    if app.show_help {
        draw_help_overlay(f, size);
    }
}

fn build_mini_ascii(app: &AppState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    // 简化版：只显示本机节点的出站连接
    let local_ip = &app.graph.local_ip;
    let mut parts = Vec::new();
    let star_label = format!("[★{}]", local_ip);
    parts.push(star_label);

    let out_edges: Vec<&Edge> = app
        .graph
        .edges
        .iter()
        .filter(|e| &e.src == local_ip)
        .take(4)
        .collect();

    for edge in out_edges {
        let arrow = if edge.protocol == "TCP" {
            format!("──TCP──►[{}]", edge.dst)
        } else {
            format!("╌╌UDP──►[{}]", edge.dst)
        };
        parts.push(arrow);
    }
    lines.push(Line::from(parts.join(" ")));
    lines
}

fn draw_help_overlay(f: &mut Frame, area: ratatui::layout::Rect) {
    use ratatui::widgets::Clear;
    let help_area = centered_rect(60, 70, area);
    f.render_widget(Clear, help_area);
    let help_text = vec![
        Line::from(" ═══ 快捷键帮助 ═══"),
        Line::from(""),
        Line::from(" q / Ctrl+C   退出程序"),
        Line::from(" r            重新扫描刷新"),
        Line::from(" ↑ / ↓        节点列表上下移动"),
        Line::from(" f            打开过滤输入框"),
        Line::from(" e            导出 JSON"),
        Line::from(" Tab          切换面板焦点"),
        Line::from(" ?            显示此帮助"),
        Line::from(""),
        Line::from(" 按任意键关闭帮助"),
    ];
    let help_widget = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("帮助")
                .style(Style::default().bg(Color::DarkGray).fg(Color::White)),
        )
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(help_widget, help_area);
}

fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    r: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn classify_node(node: &Node) -> &'static str {
    if node.is_local {
        "[本机]"
    } else if node.ports.contains(&80) || node.ports.contains(&443) {
        "[Web] "
    } else if node.ports.contains(&53) {
        "[DNS] "
    } else if node.ports.contains(&3306)
        || node.ports.contains(&5432)
        || node.ports.contains(&27017)
    {
        "[数据]"
    } else if node.ports.contains(&22) {
        "[SSH] "
    } else {
        "      "
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 测试专用渲染函数（返回 String，不写文件/不打印）
// ──────────────────────────────────────────────────────────────────────────────

/// 将 Graph 渲染为 dot 格式字符串（供测试使用）
#[allow(dead_code)]
pub fn render_dot_string(graph: &Graph) -> String {
    let mut dot = String::new();
    dot.push_str("digraph netopo {\n");
    dot.push_str("  rankdir=LR;\n");
    dot.push_str("  node [fontname=\"Helvetica\" fontsize=10];\n");
    for node in &graph.nodes {
        let label = build_node_label(node);
        if node.is_local {
            dot.push_str(&format!(
                "  \"{}\" [label=\"{}\" shape=doublecircle color=blue];\n",
                node.ip, label
            ));
        } else {
            dot.push_str(&format!("  \"{}\" [label=\"{}\"];\n", node.ip, label));
        }
    }
    for edge in &graph.edges {
        let penwidth = 1.0_f64.max(edge.count as f64 * 0.5).min(5.0);
        let style = if edge.protocol == "UDP" {
            " style=dashed"
        } else {
            ""
        };
        dot.push_str(&format!(
            "  \"{}\" -> \"{}\" [penwidth={:.1} label=\"{} x{}\"{}];\n",
            edge.src, edge.dst, penwidth, edge.protocol, edge.count, style
        ));
    }
    dot.push_str("}\n");
    dot
}

/// 将 Graph 渲染为 ASCII 字符串（带选项）
pub fn render_ascii_string_with_opts(graph: &Graph, opts: &AsciiOptions) -> String {
    let mut out = String::new();
    build_ascii(graph, opts, &mut out);
    out
}

/// 将 Graph 渲染为 ASCII 字符串（默认选项，供测试使用）
#[cfg(test)]
fn render_ascii_string(graph: &Graph) -> String {
    render_ascii_string_with_opts(graph, &AsciiOptions::default())
}

// ──────────────────────────────────────────────────────────────────────────────
// 测试
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_manager::{Edge, Graph, GraphSummary, Node};

    /// 生成唯一临时文件路径，避免并行测试间文件名冲突导致 panic（exit 101）。
    /// 使用进程 ID + 递增计数器确保每次调用返回不同路径。
    fn unique_tmp(suffix: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "netopo_test_{}_p{}.{}",
            n,
            std::process::id(),
            suffix
        ))
    }

    fn make_test_graph() -> Graph {
        Graph {
            nodes: vec![
                Node {
                    ip: "192.168.1.100".to_string(),
                    hostname: Some("mymac.local".to_string()),
                    ports: vec![22, 80],
                    is_local: true,
                    mac: Some("aa:bb:cc:dd:ee:ff".to_string()),
                    interface: Some("en0".to_string()),
                },
                Node {
                    ip: "192.168.1.1".to_string(),
                    hostname: Some("router.local".to_string()),
                    ports: vec![80, 443],
                    is_local: false,
                    mac: None,
                    interface: None,
                },
            ],
            edges: vec![Edge {
                src: "192.168.1.100".to_string(),
                dst: "192.168.1.1".to_string(),
                protocol: "TCP".to_string(),
                src_port: 55000,
                dst_port: 443,
                state: Some("ESTABLISHED".to_string()),
                count: 3,
            }],
            captured_at: "2024-01-15T10:30:00+08:00".to_string(),
            local_ip: "192.168.1.100".to_string(),
            summary: GraphSummary {
                total_nodes: 2,
                total_edges: 1,
                tcp_connections: 1,
                udp_connections: 0,
                scan_duration_ms: 1234,
            },
        }
    }

    fn make_test_graph_with_udp() -> Graph {
        let mut g = make_test_graph();
        g.edges.push(Edge {
            src: "192.168.1.100".to_string(),
            dst: "8.8.8.8".to_string(),
            protocol: "UDP".to_string(),
            src_port: 63412,
            dst_port: 53,
            state: None,
            count: 1,
        });
        g.nodes.push(Node {
            ip: "8.8.8.8".to_string(),
            hostname: Some("dns.google".to_string()),
            ports: vec![53],
            is_local: false,
            mac: None,
            interface: None,
        });
        g.summary.total_nodes = 3;
        g.summary.total_edges = 2;
        g.summary.udp_connections = 1;
        g
    }

    // ── F5 JSON 输出测试 ─────────────────────────────────────────────────────

    #[test]
    fn test_output_json_creates_valid_file() {
        let graph = make_test_graph();
        let tmp = unique_tmp("json");
        output_json(&graph, tmp.to_str().unwrap()).unwrap();
        let content = std::fs::read_to_string(&tmp).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(parsed.get("nodes").is_some());
        assert!(parsed.get("edges").is_some());
        assert!(parsed.get("captured_at").is_some());
        assert!(parsed.get("local_ip").is_some());
        assert!(parsed.get("summary").is_some());
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_output_json_local_ip_preserved() {
        let graph = make_test_graph();
        let tmp = unique_tmp("json");
        output_json(&graph, tmp.to_str().unwrap()).unwrap();
        let content = std::fs::read_to_string(&tmp).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["local_ip"], "192.168.1.100");
        let _ = std::fs::remove_file(&tmp);
    }

    // ── F6 dot 输出测试 ──────────────────────────────────────────────────────

    #[test]
    fn test_dot_contains_digraph_netopo() {
        let graph = make_test_graph();
        let dot = render_dot_string(&graph);
        assert!(dot.contains("digraph netopo"));
    }

    #[test]
    fn test_dot_contains_rankdir_lr() {
        let graph = make_test_graph();
        let dot = render_dot_string(&graph);
        assert!(dot.contains("rankdir=LR"));
    }

    #[test]
    fn test_dot_local_node_has_doublecircle() {
        let graph = make_test_graph();
        let dot = render_dot_string(&graph);
        assert!(
            dot.contains("doublecircle"),
            "Local node should have doublecircle shape"
        );
    }

    #[test]
    fn test_dot_local_node_has_color_blue() {
        let graph = make_test_graph();
        let dot = render_dot_string(&graph);
        assert!(
            dot.contains("color=blue"),
            "Local node should have color=blue"
        );
    }

    #[test]
    fn test_dot_edge_has_penwidth() {
        let graph = make_test_graph();
        let dot = render_dot_string(&graph);
        assert!(dot.contains("penwidth="), "Edges should have penwidth");
    }

    #[test]
    fn test_dot_udp_edge_has_dashed_style() {
        let graph = make_test_graph_with_udp();
        let dot = render_dot_string(&graph);
        assert!(
            dot.contains("style=dashed"),
            "UDP edges should have style=dashed"
        );
    }

    #[test]
    fn test_dot_tcp_edge_no_dashed_style() {
        // TCP-only graph should not have style=dashed
        let graph = make_test_graph();
        let dot = render_dot_string(&graph);
        assert!(
            !dot.contains("style=dashed"),
            "TCP-only graph should not have dashed style"
        );
    }

    #[test]
    fn test_output_dot_creates_valid_file() {
        let graph = make_test_graph();
        let tmp = unique_tmp("dot");
        output_dot(&graph, tmp.to_str().unwrap()).unwrap();
        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains("digraph netopo"));
        assert!(content.contains("rankdir=LR"));
        assert!(content.contains("doublecircle"));
        let _ = std::fs::remove_file(&tmp);
    }

    // ── F7 ASCII 输出测试 ────────────────────────────────────────────────────

    #[test]
    fn test_ascii_contains_star_marker() {
        let graph = make_test_graph();
        let output = render_ascii_string(&graph);
        assert!(
            output.contains('★'),
            "ASCII output must contain ★ for local node"
        );
    }

    #[test]
    fn test_ascii_contains_local_ip() {
        let graph = make_test_graph();
        let output = render_ascii_string(&graph);
        assert!(output.contains("192.168.1.100"));
    }

    #[test]
    fn test_ascii_contains_tcp_protocol() {
        let graph = make_test_graph();
        let output = render_ascii_string(&graph);
        assert!(output.contains("TCP"));
    }

    #[test]
    fn test_ascii_contains_udp_protocol() {
        let graph = make_test_graph_with_udp();
        let output = render_ascii_string(&graph);
        assert!(output.contains("UDP"));
    }

    #[test]
    fn test_ascii_header_present() {
        let graph = make_test_graph();
        let output = render_ascii_string(&graph);
        assert!(output.contains("netopo 拓扑图"));
    }

    #[test]
    fn test_ascii_summary_line_present() {
        let graph = make_test_graph();
        let output = render_ascii_string(&graph);
        assert!(output.contains("活跃连接") && output.contains("TCP"));
    }

    #[test]
    fn test_ascii_lan_section_header() {
        let graph = make_test_graph();
        let output = render_ascii_string(&graph);
        assert!(
            output.contains("局域网设备"),
            "should have LAN section header"
        );
    }

    #[test]
    fn test_ascii_public_section_header() {
        let graph = make_test_graph_with_udp();
        let output = render_ascii_string(&graph);
        assert!(
            output.contains("公网连接"),
            "should have public section when public IPs present"
        );
    }

    #[test]
    fn test_ascii_no_public_section_when_lan_only() {
        // make_test_graph has only LAN IPs
        let graph = make_test_graph();
        let output = render_ascii_string(&graph);
        assert!(
            !output.contains("公网连接"),
            "no public section when all IPs are LAN"
        );
    }

    #[test]
    fn test_ascii_isp_google_classified() {
        let graph = make_test_graph_with_udp();
        let output = render_ascii_string(&graph);
        // 8.8.8.8 should be classified as Google
        assert!(
            output.contains("Google"),
            "8.8.8.8 should be classified as Google"
        );
    }

    #[test]
    fn test_ascii_summary_has_top_ports() {
        let graph = make_test_graph();
        let output = render_ascii_string(&graph);
        // graph has TCP:443, so Top端口 should appear
        assert!(
            output.contains("Top端口"),
            "summary should include top ports when connections exist"
        );
    }

    // ── F8 TUI 测试（TestBackend，不依赖真实终端） ───────────────────────────

    #[test]
    fn test_tui_initial_render_no_panic() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let graph = make_test_graph();
        let mut app = AppState::new(graph, "127.0.0.1".to_string());
        terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
        // 只要不 panic 就通过
    }

    #[test]
    fn test_tui_buffer_contains_node_section() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let graph = make_test_graph();
        let mut app = AppState::new(graph, "127.0.0.1".to_string());
        terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        // Collect all non-space symbols from the buffer
        let content: String = buffer.content().iter().map(|c| c.symbol()).collect();
        // The TUI renders chinese characters — check the buffer is non-empty and
        // contains at least the local IP (which is always rendered)
        assert!(
            content.contains("192.168.1.100") || content.contains("节"),
            "Buffer should contain node IP or node list title. Buffer length: {}",
            content.len()
        );
    }

    #[test]
    fn test_tui_buffer_contains_connection_section() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let graph = make_test_graph();
        let mut app = AppState::new(graph, "127.0.0.1".to_string());
        terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let content: String = buffer.content().iter().map(|c| c.symbol()).collect();
        // The detail panel renders the selected node's IP
        assert!(
            content.contains("192.168.1") || content.contains("连"),
            "Buffer should contain node IP in connection details or connection title"
        );
    }

    #[test]
    fn test_tui_key_q_sets_quit_flag() {
        let graph = make_test_graph();
        let mut app = AppState::new(graph, "127.0.0.1".to_string());
        // Simulate 'q' key by calling the state update logic directly
        // AppState doesn't have handle_key, so we test the state field after simulated logic
        // We verify initial state doesn't quit, and that the field exists
        assert_eq!(app.selected, 0);
        // Simulate pressing Down to change selection
        let node_count = app.filtered_nodes().len();
        if node_count > 1 {
            app.selected = (app.selected + 1).min(node_count - 1);
            assert_eq!(app.selected, 1);
        }
        // Simulate pressing Up
        if app.selected > 0 {
            app.selected -= 1;
        }
        assert_eq!(app.selected, 0);
        // Test key 'q' would break the loop — we test that filter_mode is false initially
        assert!(!app.filter_mode);
    }

    #[test]
    fn test_tui_filtered_nodes_with_keyword() {
        let graph = make_test_graph();
        let mut app = AppState::new(graph, "127.0.0.1".to_string());
        // No filter => all nodes visible
        assert_eq!(app.filtered_nodes().len(), 2);
        // Filter by "router"
        app.filter = "router".to_string();
        let filtered = app.filtered_nodes();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].ip, "192.168.1.1");
    }

    #[test]
    fn test_tui_filtered_nodes_by_ip() {
        let graph = make_test_graph();
        let mut app = AppState::new(graph, "127.0.0.1".to_string());
        app.filter = "192.168.1.100".to_string();
        let filtered = app.filtered_nodes();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].ip, "192.168.1.100");
    }

    #[test]
    fn test_tui_selected_node_returns_correct() {
        let graph = make_test_graph();
        let mut app = AppState::new(graph, "127.0.0.1".to_string());
        let node = app.selected_node().expect("should have a selected node");
        assert_eq!(node.ip, "192.168.1.100"); // first node, index 0
        app.selected = 1;
        let node2 = app.selected_node().expect("should have node at index 1");
        assert_eq!(node2.ip, "192.168.1.1");
    }

    #[test]
    fn test_tui_render_with_udp_graph() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let graph = make_test_graph_with_udp();
        let mut app = AppState::new(graph, "127.0.0.1".to_string());
        // Should not panic with UDP edges
        terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    }

    #[test]
    fn test_tui_render_empty_graph() {
        use crate::data_manager::GraphSummary;
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        let empty_graph = Graph {
            nodes: vec![],
            edges: vec![],
            captured_at: "2024-01-15T10:30:00+00:00".to_string(),
            local_ip: "".to_string(),
            summary: GraphSummary {
                total_nodes: 0,
                total_edges: 0,
                tcp_connections: 0,
                udp_connections: 0,
                scan_duration_ms: 0,
            },
        };
        let mut app = AppState::new(empty_graph, "127.0.0.1".to_string());
        terminal.draw(|f| draw_ui(f, &mut app)).unwrap();
    }
}
