//! cli — clap 参数定义，所有 --flag 在此声明

use clap::Parser;

/// netopo — 本地网络拓扑探测与可视化工具
#[derive(Debug, Parser)]
#[command(
    name = "netopo",
    version,
    about = "本地网络拓扑探测与可视化工具",
    long_about = None
)]
pub struct Cli {
    // ── 扫描选项 ──────────────────────────────────────────────────────────────
    /// 扫描局域网设备（自动检测主接口子网）
    #[arg(long)]
    pub scan: bool,

    /// 指定扫描网段，如 10.0.0.0/24
    #[arg(long, value_name = "CIDR")]
    pub subnet: Option<String>,

    /// 指定端口，如 "22,80,443" 或 "1-1024"
    #[arg(long, value_name = "LIST")]
    pub ports: Option<String>,

    /// 并发连接数，默认 256
    #[arg(long, value_name = "N", default_value = "256")]
    pub concurrency: usize,

    /// 单连接超时毫秒，默认 500
    #[arg(long, value_name = "MS", default_value = "500")]
    pub timeout: u64,

    // ── 连接选项 ──────────────────────────────────────────────────────────────
    /// 获取当前 TCP/UDP 连接快照
    #[arg(long)]
    pub connections: bool,

    /// 只显示局域网内连接
    #[arg(long)]
    pub local_only: bool,

    /// 排除 loopback 连接
    #[arg(long)]
    pub exclude_loopback: bool,

    /// 每 N 秒刷新，默认 5
    #[arg(long, value_name = "SECONDS")]
    pub watch: Option<u64>,

    // ── 图构建 ────────────────────────────────────────────────────────────────
    /// 构建拓扑图
    #[arg(long)]
    pub graph: bool,

    /// 只显示连接数 >= N 的节点
    #[arg(long, value_name = "N")]
    pub min_connections: Option<u32>,

    // ── 输出格式 ──────────────────────────────────────────────────────────────
    /// 输出 JSON 文件
    #[arg(long, value_name = "FILE")]
    pub output_json: Option<String>,

    /// 输出 Graphviz dot 文件
    #[arg(long, value_name = "FILE")]
    pub output_dot: Option<String>,

    /// 终端打印 ASCII 拓扑图
    #[arg(long)]
    pub ascii: bool,

    /// 启动 TUI 交互界面
    #[arg(long)]
    pub tui: bool,

    // ── 通用 ──────────────────────────────────────────────────────────────────
    /// 详细输出
    #[arg(short, long)]
    pub verbose: bool,

    /// 静默模式
    #[arg(short, long)]
    pub quiet: bool,
}

impl Cli {
    /// 解析用户指定的端口字符串，支持 "22,80,443" 和 "1-1024" 两种格式。
    pub fn parse_ports(&self) -> anyhow::Result<Vec<u16>> {
        let default_ports: Vec<u16> = vec![
            21, 22, 23, 25, 53, 80, 110, 143, 443, 445, 3306, 3389, 5432, 5900, 6379, 8080, 8443,
            8888, 9200, 27017,
        ];
        let Some(ref spec) = self.ports else {
            return Ok(default_ports);
        };
        let mut result = Vec::new();
        for part in spec.split(',') {
            let part = part.trim();
            if let Some((lo, hi)) = part.split_once('-') {
                let lo: u16 = lo
                    .trim()
                    .parse()
                    .map_err(|_| anyhow::anyhow!("无效端口范围: {}", part))?;
                let hi: u16 = hi
                    .trim()
                    .parse()
                    .map_err(|_| anyhow::anyhow!("无效端口范围: {}", part))?;
                for p in lo..=hi {
                    result.push(p);
                }
            } else {
                let p: u16 = part
                    .parse()
                    .map_err(|_| anyhow::anyhow!("无效端口: {}", part))?;
                result.push(p);
            }
        }
        if result.is_empty() {
            return Ok(default_ports);
        }
        Ok(result)
    }
}
