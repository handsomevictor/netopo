//! scanner — F1: 网卡枚举；F2: 子网扫描 + 端口探测 + DNS 反向解析

use crate::data_manager::Node;
use anyhow::Context;
use ipnet::IpNet;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::Semaphore;

// ──────────────────────────────────────────────────────────────────────────────
// F1 — 本机网卡扫描
// ──────────────────────────────────────────────────────────────────────────────

/// 枚举本机所有网络接口，返回 Node 列表。
/// 主接口（有默认路由的）会设置 is_local = true。
pub async fn scan_interfaces() -> anyhow::Result<Vec<Node>> {
    let interfaces = pnet::datalink::interfaces();
    let primary = detect_primary_interface();

    let mut nodes = Vec::new();
    for iface in &interfaces {
        for ip_net in &iface.ips {
            let ip = ip_net.ip();
            // 跳过 IPv6（保持简洁；IPv6 支持可后续扩展）
            if ip.is_ipv6() {
                continue;
            }
            let mac = iface.mac.map(|m| m.to_string());
            let is_primary = primary.as_deref() == Some(iface.name.as_str());
            nodes.push(Node {
                ip: ip.to_string(),
                hostname: None,
                ports: Vec::new(),
                is_local: is_primary,
                mac,
                interface: Some(iface.name.clone()),
            });
        }
    }
    Ok(nodes)
}

/// 检测"主接口"名称（通过解析默认路由）。
/// macOS: `route -n get default`
/// Linux: `ip route show default`
fn detect_primary_interface() -> Option<String> {
    // macOS
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("route")
            .args(["-n", "get", "default"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let line = line.trim();
                if line.starts_with("interface:") {
                    return line.split_once(':').map(|x| x.1.trim().to_string());
                }
            }
        }
    }

    // Linux
    #[cfg(target_os = "linux")]
    {
        if let Ok(out) = std::process::Command::new("ip")
            .args(["route", "show", "default"])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            // "default via ... dev eth0 ..."
            for token_pair in text.split_whitespace().collect::<Vec<_>>().windows(2) {
                if token_pair[0] == "dev" {
                    return Some(token_pair[1].to_string());
                }
            }
        }
    }

    None
}

// ──────────────────────────────────────────────────────────────────────────────
// F2 — 局域网设备扫描
// ──────────────────────────────────────────────────────────────────────────────

const DEFAULT_PORTS: &[u16] = &[
    21, 22, 23, 25, 53, 80, 110, 143, 443, 445, 3306, 3389, 5432, 5900, 6379, 8080, 8443, 8888,
    9200, 27017,
];

/// 扫描指定 CIDR 内所有存活主机，探测端口，反向 DNS 解析。
pub async fn scan_subnet(
    cidr: &str,
    ports: &[u16],
    concurrency: usize,
    timeout_ms: u64,
) -> anyhow::Result<Vec<Node>> {
    let ports = if ports.is_empty() {
        DEFAULT_PORTS
    } else {
        ports
    };
    let network: IpNet = cidr
        .parse()
        .with_context(|| format!("无效 CIDR: {}", cidr))?;

    let hosts: Vec<IpAddr> = network.hosts().collect();
    let timeout = Duration::from_millis(timeout_ms);
    let semaphore = Arc::new(Semaphore::new(concurrency));

    // 先并发探测哪些主机存活（在默认端口上至少有一个开放）
    let mut tasks = Vec::new();
    for host in hosts {
        let ports_owned: Vec<u16> = ports.to_vec();
        let sem = semaphore.clone();
        tasks.push(tokio::spawn(async move {
            probe_host(host, &ports_owned, timeout, sem).await
        }));
    }

    let mut nodes = Vec::new();
    for task in tasks {
        match task.await {
            Ok(Ok(Some(node))) => nodes.push(node),
            Ok(Ok(None)) => {}
            Ok(Err(e)) => {
                if !e.to_string().contains("Connection refused")
                    && !e.to_string().contains("timed out")
                {
                    eprintln!("探测错误: {e}");
                }
            }
            Err(e) => eprintln!("任务 panic: {e}"),
        }
    }
    Ok(nodes)
}

/// 探测单个主机：尝试连接所有端口，记录开放端口，解析 hostname。
async fn probe_host(
    ip: IpAddr,
    ports: &[u16],
    timeout: Duration,
    semaphore: Arc<Semaphore>,
) -> anyhow::Result<Option<Node>> {
    let mut open_ports = Vec::new();

    // 对每个端口并发 TCP connect
    let mut port_tasks = Vec::new();
    for &port in ports {
        let sem = semaphore.clone();
        port_tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let addr = SocketAddr::new(ip, port);
            let result = tokio::time::timeout(timeout, TcpStream::connect(addr)).await;
            (port, matches!(result, Ok(Ok(_))))
        }));
    }
    for task in port_tasks {
        if let Ok((port, true)) = task.await {
            open_ports.push(port);
        }
    }

    if open_ports.is_empty() {
        return Ok(None);
    }

    // DNS 反向解析
    let hostname = reverse_lookup(ip).await;

    Ok(Some(Node {
        ip: ip.to_string(),
        hostname,
        ports: open_ports,
        is_local: false,
        mac: None,
        interface: None,
    }))
}

/// 异步 DNS 反向解析（dns-lookup 是同步的，用 spawn_blocking 包装）
async fn reverse_lookup(ip: IpAddr) -> Option<String> {
    tokio::task::spawn_blocking(move || dns_lookup::lookup_addr(&ip).ok())
        .await
        .ok()
        .flatten()
}

// ──────────────────────────────────────────────────────────────────────────────
// 辅助：将本机接口 IP 转换为子网 CIDR（供自动检测时使用）
// ──────────────────────────────────────────────────────────────────────────────

/// 根据 pnet 枚举到的接口 IP，返回其所在的 /24 CIDR。
pub fn default_cidr() -> Option<String> {
    let primary = detect_primary_interface()?;
    let interfaces = pnet::datalink::interfaces();
    for iface in &interfaces {
        if iface.name != primary {
            continue;
        }
        for ip_net in &iface.ips {
            if ip_net.ip().is_ipv4() && !ip_net.ip().is_loopback() {
                // 返回该网段的 CIDR
                return Some(ip_net.to_string());
            }
        }
    }
    None
}

/// 返回主接口的本机 IP 字符串（用于标记 local_ip）
pub fn local_ip() -> Option<String> {
    let primary = detect_primary_interface()?;
    let interfaces = pnet::datalink::interfaces();
    for iface in &interfaces {
        if iface.name != primary {
            continue;
        }
        for ip_net in &iface.ips {
            if ip_net.ip().is_ipv4() && !ip_net.ip().is_loopback() {
                return Some(ip_net.ip().to_string());
            }
        }
    }
    None
}

// ──────────────────────────────────────────────────────────────────────────────
// 辅助：端口字符串解析 和 CIDR 主机地址枚举
// ──────────────────────────────────────────────────────────────────────────────

/// 解析端口列表字符串，支持逗号分隔和范围（如 "22,80,8080-8082"）
///
/// 返回 u16 端口列表，若格式非法或端口超过 65535 则返回 Error。
#[allow(dead_code)]
pub fn parse_ports(s: &str) -> anyhow::Result<Vec<u16>> {
    let mut ports = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.contains('-') {
            let mut iter = part.splitn(2, '-');
            let start_str = iter.next().unwrap_or("").trim();
            let end_str = iter.next().unwrap_or("").trim();
            let start: u32 = start_str
                .parse()
                .map_err(|_| anyhow::anyhow!("无效端口: {}", start_str))?;
            let end: u32 = end_str
                .parse()
                .map_err(|_| anyhow::anyhow!("无效端口: {}", end_str))?;
            if start > 65535 || end > 65535 {
                anyhow::bail!("端口超出范围 (0-65535): {}-{}", start, end);
            }
            if start > end {
                anyhow::bail!("范围起点大于终点: {}-{}", start, end);
            }
            for p in start..=end {
                ports.push(p as u16);
            }
        } else {
            let p: u32 = part
                .parse()
                .map_err(|_| anyhow::anyhow!("无效端口: {}", part))?;
            if p > 65535 {
                anyhow::bail!("端口超出范围 (0-65535): {}", p);
            }
            ports.push(p as u16);
        }
    }
    Ok(ports)
}

/// 将 CIDR 字符串枚举为所有主机地址（排除网络地址和广播地址）
///
/// 例如 "192.168.1.0/24" 返回 192.168.1.1 ~ 192.168.1.254（254 个地址）
#[allow(dead_code)]
pub fn cidr_to_host_ips(cidr: &str) -> anyhow::Result<Vec<IpAddr>> {
    let network: IpNet = cidr
        .parse()
        .map_err(|_| anyhow::anyhow!("无效 CIDR: {}", cidr))?;
    Ok(network.hosts().collect())
}

// ──────────────────────────────────────────────────────────────────────────────
// 测试
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_ports 测试 ─────────────────────────────────────────────────────

    #[test]
    fn test_parse_ports_comma_separated() {
        let ports = parse_ports("22,80,443").unwrap();
        assert_eq!(ports, vec![22, 80, 443]);
    }

    #[test]
    fn test_parse_ports_range() {
        let ports = parse_ports("1-5").unwrap();
        assert_eq!(ports, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_parse_ports_mixed() {
        let ports = parse_ports("22,80,8080-8082").unwrap();
        assert_eq!(ports, vec![22, 80, 8080, 8081, 8082]);
    }

    #[test]
    fn test_parse_ports_single() {
        let ports = parse_ports("443").unwrap();
        assert_eq!(ports, vec![443]);
    }

    #[test]
    fn test_parse_ports_invalid_alpha_returns_error() {
        assert!(parse_ports("abc").is_err());
    }

    #[test]
    fn test_parse_ports_out_of_range_returns_error() {
        assert!(parse_ports("99999").is_err());
    }

    #[test]
    fn test_parse_ports_range_out_of_range_returns_error() {
        assert!(parse_ports("1-99999").is_err());
    }

    #[test]
    fn test_parse_ports_reversed_range_returns_error() {
        assert!(parse_ports("443-80").is_err());
    }

    #[test]
    fn test_parse_ports_max_valid_port() {
        let ports = parse_ports("65535").unwrap();
        assert_eq!(ports, vec![65535]);
    }

    #[test]
    fn test_parse_ports_zero() {
        let ports = parse_ports("0").unwrap();
        assert_eq!(ports, vec![0]);
    }

    #[test]
    fn test_parse_ports_range_single_element() {
        let ports = parse_ports("80-80").unwrap();
        assert_eq!(ports, vec![80]);
    }

    // ── cidr_to_host_ips 测试 ────────────────────────────────────────────────

    #[test]
    fn test_cidr_slash24_has_254_hosts() {
        let ips = cidr_to_host_ips("192.168.1.0/24").unwrap();
        assert_eq!(ips.len(), 254, "A /24 should have 254 host addresses");
    }

    #[test]
    fn test_cidr_slash24_contains_first_host() {
        let ips = cidr_to_host_ips("192.168.1.0/24").unwrap();
        let first: IpAddr = "192.168.1.1".parse().unwrap();
        assert!(ips.contains(&first));
    }

    #[test]
    fn test_cidr_slash24_contains_last_host() {
        let ips = cidr_to_host_ips("192.168.1.0/24").unwrap();
        let last: IpAddr = "192.168.1.254".parse().unwrap();
        assert!(ips.contains(&last));
    }

    #[test]
    fn test_cidr_slash24_excludes_network_address() {
        let ips = cidr_to_host_ips("192.168.1.0/24").unwrap();
        let network: IpAddr = "192.168.1.0".parse().unwrap();
        assert!(
            !ips.contains(&network),
            "Network address should be excluded"
        );
    }

    #[test]
    fn test_cidr_slash24_excludes_broadcast_address() {
        let ips = cidr_to_host_ips("192.168.1.0/24").unwrap();
        let broadcast: IpAddr = "192.168.1.255".parse().unwrap();
        assert!(
            !ips.contains(&broadcast),
            "Broadcast address should be excluded"
        );
    }

    #[test]
    fn test_cidr_slash30_has_2_hosts() {
        let ips = cidr_to_host_ips("10.0.0.0/30").unwrap();
        assert_eq!(ips.len(), 2, "A /30 should have 2 host addresses");
    }

    #[test]
    fn test_cidr_slash32_has_1_host() {
        let ips = cidr_to_host_ips("127.0.0.1/32").unwrap();
        assert_eq!(ips.len(), 1, "A /32 should have exactly 1 address");
    }

    #[test]
    fn test_cidr_invalid_returns_error() {
        assert!(cidr_to_host_ips("not-a-cidr").is_err());
        assert!(cidr_to_host_ips("256.0.0.0/24").is_err());
        assert!(cidr_to_host_ips("192.168.1.0/33").is_err());
    }

    #[test]
    fn test_cidr_slash16_has_65534_hosts() {
        let ips = cidr_to_host_ips("10.0.0.0/16").unwrap();
        assert_eq!(ips.len(), 65534, "A /16 should have 65534 host addresses");
    }
}
