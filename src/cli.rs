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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// 构造一个最小化的 Cli，只设置指定字段，其余使用默认值。
    fn parse_args(args: &[&str]) -> Cli {
        // 第一个参数是程序名
        let mut full_args = vec!["netopo"];
        full_args.extend_from_slice(args);
        Cli::parse_from(full_args)
    }

    // ── --scan flag ──────────────────────────────────────────────────────────

    #[test]
    fn test_scan_flag_default_false() {
        let cli = parse_args(&[]);
        assert!(!cli.scan);
    }

    #[test]
    fn test_scan_flag_set_true() {
        let cli = parse_args(&["--scan"]);
        assert!(cli.scan);
    }

    // ── --ports ──────────────────────────────────────────────────────────────

    #[test]
    fn test_ports_none_by_default() {
        let cli = parse_args(&[]);
        assert!(cli.ports.is_none());
    }

    #[test]
    fn test_ports_comma_separated_parsed() {
        let cli = parse_args(&["--ports", "22,80,443"]);
        let ports = cli.parse_ports().unwrap();
        assert_eq!(ports, vec![22, 80, 443]);
    }

    #[test]
    fn test_ports_range_parsed() {
        let cli = parse_args(&["--ports", "8080-8082"]);
        let ports = cli.parse_ports().unwrap();
        assert_eq!(ports, vec![8080, 8081, 8082]);
    }

    #[test]
    fn test_ports_mixed_format() {
        let cli = parse_args(&["--ports", "22,80,8080-8082"]);
        let ports = cli.parse_ports().unwrap();
        assert_eq!(ports, vec![22, 80, 8080, 8081, 8082]);
    }

    #[test]
    fn test_parse_ports_returns_default_when_none() {
        let cli = parse_args(&[]);
        let ports = cli.parse_ports().unwrap();
        // 默认端口列表包含 22、80、443
        assert!(ports.contains(&22));
        assert!(ports.contains(&80));
        assert!(ports.contains(&443));
        assert_eq!(ports.len(), 20);
    }

    #[test]
    fn test_parse_ports_invalid_returns_error() {
        let cli = parse_args(&["--ports", "abc"]);
        assert!(cli.parse_ports().is_err());
    }

    // ── --subnet ─────────────────────────────────────────────────────────────

    #[test]
    fn test_subnet_none_by_default() {
        let cli = parse_args(&[]);
        assert!(cli.subnet.is_none());
    }

    #[test]
    fn test_subnet_value_set() {
        let cli = parse_args(&["--subnet", "192.168.1.0/24"]);
        assert_eq!(cli.subnet.as_deref(), Some("192.168.1.0/24"));
    }

    #[test]
    fn test_subnet_arbitrary_cidr() {
        let cli = parse_args(&["--subnet", "10.0.0.0/8"]);
        assert_eq!(cli.subnet.as_deref(), Some("10.0.0.0/8"));
    }

    // ── --concurrency ────────────────────────────────────────────────────────

    #[test]
    fn test_concurrency_default_256() {
        let cli = parse_args(&[]);
        assert_eq!(cli.concurrency, 256);
    }

    #[test]
    fn test_concurrency_custom_value() {
        let cli = parse_args(&["--concurrency", "128"]);
        assert_eq!(cli.concurrency, 128);
    }

    #[test]
    fn test_concurrency_custom_512() {
        let cli = parse_args(&["--concurrency", "512"]);
        assert_eq!(cli.concurrency, 512);
    }

    // ── --timeout ────────────────────────────────────────────────────────────

    #[test]
    fn test_timeout_default_500() {
        let cli = parse_args(&[]);
        assert_eq!(cli.timeout, 500);
    }

    #[test]
    fn test_timeout_custom_value() {
        let cli = parse_args(&["--timeout", "1000"]);
        assert_eq!(cli.timeout, 1000);
    }

    // ── --connections / --local-only / --exclude-loopback ────────────────────

    #[test]
    fn test_connections_flag_default_false() {
        let cli = parse_args(&[]);
        assert!(!cli.connections);
    }

    #[test]
    fn test_connections_flag_set_true() {
        let cli = parse_args(&["--connections"]);
        assert!(cli.connections);
    }

    #[test]
    fn test_local_only_default_false() {
        let cli = parse_args(&[]);
        assert!(!cli.local_only);
    }

    #[test]
    fn test_local_only_set_true() {
        let cli = parse_args(&["--local-only"]);
        assert!(cli.local_only);
    }

    #[test]
    fn test_exclude_loopback_default_false() {
        let cli = parse_args(&[]);
        assert!(!cli.exclude_loopback);
    }

    #[test]
    fn test_exclude_loopback_set_true() {
        let cli = parse_args(&["--exclude-loopback"]);
        assert!(cli.exclude_loopback);
    }

    // ── --watch ──────────────────────────────────────────────────────────────

    #[test]
    fn test_watch_none_by_default() {
        let cli = parse_args(&[]);
        assert!(cli.watch.is_none());
    }

    #[test]
    fn test_watch_value_set() {
        let cli = parse_args(&["--watch", "10"]);
        assert_eq!(cli.watch, Some(10));
    }

    // ── --graph / --min-connections ──────────────────────────────────────────

    #[test]
    fn test_graph_flag_default_false() {
        let cli = parse_args(&[]);
        assert!(!cli.graph);
    }

    #[test]
    fn test_graph_flag_set_true() {
        let cli = parse_args(&["--graph"]);
        assert!(cli.graph);
    }

    #[test]
    fn test_min_connections_none_by_default() {
        let cli = parse_args(&[]);
        assert!(cli.min_connections.is_none());
    }

    #[test]
    fn test_min_connections_value_set() {
        let cli = parse_args(&["--min-connections", "3"]);
        assert_eq!(cli.min_connections, Some(3));
    }

    // ── --output-json / --output-dot / --ascii / --tui ───────────────────────

    #[test]
    fn test_output_json_none_by_default() {
        let cli = parse_args(&[]);
        assert!(cli.output_json.is_none());
    }

    #[test]
    fn test_output_json_value_set() {
        let cli = parse_args(&["--output-json", "result.json"]);
        assert_eq!(cli.output_json.as_deref(), Some("result.json"));
    }

    #[test]
    fn test_output_dot_none_by_default() {
        let cli = parse_args(&[]);
        assert!(cli.output_dot.is_none());
    }

    #[test]
    fn test_output_dot_value_set() {
        let cli = parse_args(&["--output-dot", "net.dot"]);
        assert_eq!(cli.output_dot.as_deref(), Some("net.dot"));
    }

    #[test]
    fn test_ascii_flag_default_false() {
        let cli = parse_args(&[]);
        assert!(!cli.ascii);
    }

    #[test]
    fn test_ascii_flag_set_true() {
        let cli = parse_args(&["--ascii"]);
        assert!(cli.ascii);
    }

    #[test]
    fn test_tui_flag_default_false() {
        let cli = parse_args(&[]);
        assert!(!cli.tui);
    }

    #[test]
    fn test_tui_flag_set_true() {
        let cli = parse_args(&["--tui"]);
        assert!(cli.tui);
    }

    // ── --verbose / --quiet ──────────────────────────────────────────────────

    #[test]
    fn test_verbose_default_false() {
        let cli = parse_args(&[]);
        assert!(!cli.verbose);
    }

    #[test]
    fn test_verbose_set_true() {
        let cli = parse_args(&["-v"]);
        assert!(cli.verbose);
    }

    #[test]
    fn test_verbose_long_flag() {
        let cli = parse_args(&["--verbose"]);
        assert!(cli.verbose);
    }

    #[test]
    fn test_quiet_default_false() {
        let cli = parse_args(&[]);
        assert!(!cli.quiet);
    }

    #[test]
    fn test_quiet_set_true() {
        let cli = parse_args(&["-q"]);
        assert!(cli.quiet);
    }

    // ── 组合参数 ─────────────────────────────────────────────────────────────

    #[test]
    fn test_combined_scan_ascii() {
        let cli = parse_args(&["--scan", "--ascii"]);
        assert!(cli.scan);
        assert!(cli.ascii);
    }

    #[test]
    fn test_combined_scan_subnet_ports_concurrency() {
        let cli = parse_args(&[
            "--scan",
            "--subnet",
            "10.0.0.0/24",
            "--ports",
            "22,80,443",
            "--concurrency",
            "128",
        ]);
        assert!(cli.scan);
        assert_eq!(cli.subnet.as_deref(), Some("10.0.0.0/24"));
        assert_eq!(cli.concurrency, 128);
        let ports = cli.parse_ports().unwrap();
        assert_eq!(ports, vec![22, 80, 443]);
    }

    #[test]
    fn test_combined_connections_local_only_exclude_loopback() {
        let cli = parse_args(&["--connections", "--local-only", "--exclude-loopback"]);
        assert!(cli.connections);
        assert!(cli.local_only);
        assert!(cli.exclude_loopback);
    }
}
