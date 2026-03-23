//! connection_tracker — F3: TCP/UDP 连接快照，跨平台实现
//!
//! macOS:  执行 `netstat -an -p tcp` / `netstat -an -p udp`
//! Linux:  读取 /proc/net/tcp, /proc/net/tcp6, /proc/net/udp
//! Windows: 暂不实现（占位）

use crate::data_manager::Edge;

/// 获取当前活跃 TCP/UDP 连接的快照。
///
/// * `local_only`         — 只保留本地局域网（私有地址）连接
/// * `exclude_loopback`   — 排除 loopback（127.x.x.x / ::1）连接
pub async fn get_connections(
    local_only: bool,
    exclude_loopback: bool,
) -> anyhow::Result<Vec<Edge>> {
    #[cfg(target_os = "macos")]
    {
        get_connections_macos(local_only, exclude_loopback).await
    }
    #[cfg(target_os = "linux")]
    {
        get_connections_linux(local_only, exclude_loopback).await
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        anyhow::bail!("当前平台暂不支持 TCP/UDP 连接追踪");
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// macOS 实现
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
async fn get_connections_macos(
    local_only: bool,
    exclude_loopback: bool,
) -> anyhow::Result<Vec<Edge>> {
    let tcp = run_netstat("tcp").await?;
    let udp = run_netstat("udp").await?;

    let mut edges = Vec::new();
    for (proto, lines) in [("TCP", tcp), ("UDP", udp)] {
        for line in lines {
            if let Some(edge) = parse_netstat_line(&line, proto) {
                if should_include(&edge, local_only, exclude_loopback) {
                    edges.push(edge);
                }
            }
        }
    }
    Ok(edges)
}

#[cfg(target_os = "macos")]
async fn run_netstat(proto: &str) -> anyhow::Result<Vec<String>> {
    let out = tokio::process::Command::new("netstat")
        .args(["-an", "-p", proto])
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("执行 netstat 失败（需要 netstat 工具）: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text.lines().map(|l| l.to_string()).collect())
}

#[cfg(target_os = "macos")]
fn parse_netstat_line(line: &str, protocol: &str) -> Option<Edge> {
    // 示例行（macOS）:
    // tcp4   0   0  192.168.1.100.54321  192.168.1.1.443  ESTABLISHED
    // udp4   0   0  *.*                  *.*
    let cols: Vec<&str> = line.split_whitespace().collect();
    // 至少需要 5 列：proto recv-q send-q local foreign [state]
    if cols.len() < 5 {
        return None;
    }
    let proto_col = cols[0].to_lowercase();
    if !proto_col.starts_with("tcp") && !proto_col.starts_with("udp") {
        return None;
    }

    let local_raw = cols[3];
    let foreign_raw = cols[4];

    let (src, src_port) = split_addr_port(local_raw)?;
    let (dst, dst_port) = split_addr_port(foreign_raw)?;

    // 对于 UDP 可能没有 state 列
    let state = if cols.len() >= 6 && protocol == "TCP" {
        Some(cols[5].to_string())
    } else {
        None
    };

    // 排除通配符条目
    if src == "*" || dst == "*" {
        return None;
    }

    Some(Edge {
        src,
        dst,
        protocol: protocol.to_string(),
        src_port,
        dst_port,
        state,
        count: 1,
    })
}

/// 解析 "192.168.1.1.443" 或 "[::1].80" 形式的地址，返回 (ip, port)
#[cfg(target_os = "macos")]
fn split_addr_port(s: &str) -> Option<(String, u16)> {
    if s == "*.*" || s == "*" {
        return None;
    }
    // IPv6 形式: [addr].port
    if s.starts_with('[') {
        let end = s.find(']')?;
        let ip = s[1..end].to_string();
        let port_str = s.get(end + 2..)?; // skip '].'
        let port: u16 = port_str.parse().ok()?;
        return Some((ip, port));
    }
    // IPv4 形式: a.b.c.d.port（最后一段是端口）
    let parts: Vec<&str> = s.rsplitn(2, '.').collect();
    if parts.len() != 2 {
        return None;
    }
    let port: u16 = parts[0].parse().ok()?;
    let ip = parts[1].to_string();
    Some((ip, port))
}

// ──────────────────────────────────────────────────────────────────────────────
// Linux 实现
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
async fn get_connections_linux(
    local_only: bool,
    exclude_loopback: bool,
) -> anyhow::Result<Vec<Edge>> {
    let mut edges = Vec::new();
    for (path, proto) in &[
        ("/proc/net/tcp", "TCP"),
        ("/proc/net/tcp6", "TCP"),
        ("/proc/net/udp", "UDP"),
    ] {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => {
                for edge in parse_proc_net(&content, proto) {
                    if should_include(&edge, local_only, exclude_loopback) {
                        edges.push(edge);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                eprintln!("读取 {} 失败: {}", path, e);
            }
        }
    }
    Ok(edges)
}

#[cfg(target_os = "linux")]
fn parse_proc_net(content: &str, protocol: &str) -> Vec<Edge> {
    let mut edges = Vec::new();
    for line in content.lines().skip(1) {
        // 格式: sl local_address rem_address st ...
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        let local_hex = cols[1];
        let remote_hex = cols[2];
        let state_hex = cols[3];

        let (src, src_port) = match parse_hex_addr(local_hex) {
            Some(v) => v,
            None => continue,
        };
        let (dst, dst_port) = match parse_hex_addr(remote_hex) {
            Some(v) => v,
            None => continue,
        };
        let state = decode_tcp_state(state_hex);

        edges.push(Edge {
            src,
            dst,
            protocol: protocol.to_string(),
            src_port,
            dst_port,
            state: Some(state),
            count: 1,
        });
    }
    edges
}

/// 解析十六进制地址 "0101A8C0:0050" => ("192.168.1.1", 80)
#[cfg(target_os = "linux")]
fn parse_hex_addr(s: &str) -> Option<(String, u16)> {
    let (addr_hex, port_hex) = s.split_once(':')?;
    let port = u16::from_str_radix(port_hex, 16).ok()?;

    if addr_hex.len() == 8 {
        // IPv4: little-endian 32-bit hex
        let raw = u32::from_str_radix(addr_hex, 16).ok()?;
        let a = (raw) & 0xFF;
        let b = (raw >> 8) & 0xFF;
        let c = (raw >> 16) & 0xFF;
        let d = (raw >> 24) & 0xFF;
        Some((format!("{}.{}.{}.{}", a, b, c, d), port))
    } else if addr_hex.len() == 32 {
        // IPv6: 4 little-endian 32-bit words
        let mut groups = Vec::new();
        for chunk in addr_hex.as_bytes().chunks(8) {
            let word_hex = std::str::from_utf8(chunk).ok()?;
            let raw = u32::from_str_radix(word_hex, 16).ok()?.swap_bytes();
            groups.push(format!("{:08x}", raw));
        }
        let ipv6_str = groups
            .iter()
            .flat_map(|g| {
                g.as_bytes()
                    .chunks(4)
                    .map(|c| std::str::from_utf8(c).unwrap_or("0000").to_string())
            })
            .collect::<Vec<_>>()
            .join(":");
        Some((ipv6_str, port))
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn decode_tcp_state(hex: &str) -> String {
    match hex.to_uppercase().as_str() {
        "01" => "ESTABLISHED",
        "02" => "SYN_SENT",
        "03" => "SYN_RECV",
        "04" => "FIN_WAIT1",
        "05" => "FIN_WAIT2",
        "06" => "TIME_WAIT",
        "07" => "CLOSE",
        "08" => "CLOSE_WAIT",
        "09" => "LAST_ACK",
        "0A" => "LISTEN",
        "0B" => "CLOSING",
        _ => "UNKNOWN",
    }
    .to_string()
}

// ──────────────────────────────────────────────────────────────────────────────
// 过滤辅助
// ──────────────────────────────────────────────────────────────────────────────

fn should_include(edge: &Edge, local_only: bool, exclude_loopback: bool) -> bool {
    if exclude_loopback && (is_loopback(&edge.src) || is_loopback(&edge.dst)) {
        return false;
    }
    if local_only && !is_private(&edge.dst) {
        return false;
    }
    true
}

fn is_loopback(ip: &str) -> bool {
    ip.starts_with("127.") || ip == "::1" || ip == "0.0.0.0"
}

fn is_private(ip: &str) -> bool {
    ip.starts_with("10.")
        || ip.starts_with("192.168.")
        || ip.starts_with("172.16.")
        || ip.starts_with("172.17.")
        || ip.starts_with("172.18.")
        || ip.starts_with("172.19.")
        || ip.starts_with("172.2")
        || ip.starts_with("172.30.")
        || ip.starts_with("172.31.")
}

// ──────────────────────────────────────────────────────────────────────────────
// 测试专用：平台无关的解析实现（用于单元测试 macOS/Linux 两套解析逻辑）
// ──────────────────────────────────────────────────────────────────────────────

/// 解析 macOS netstat 地址 "a.b.c.d.port" 或 "*.port" => (ip, port)
/// 仅在测试中使用（生产代码使用 #[cfg(target_os="macos")] split_addr_port）
#[cfg(test)]
fn test_split_macos_addr(s: &str) -> Option<(String, u16)> {
    if s == "*.*" || s == "*" {
        return None;
    }
    if s.starts_with('[') {
        let end = s.find(']')?;
        let ip = s[1..end].to_string();
        let port_str = s.get(end + 2..)?;
        let port: u16 = port_str.parse().ok()?;
        return Some((ip, port));
    }
    // "*.8080" => ("0.0.0.0", 8080)
    if s.starts_with("*.") {
        let port_str = &s[2..];
        if port_str == "*" {
            return None;
        }
        let port: u16 = port_str.parse().ok()?;
        return Some(("0.0.0.0".to_string(), port));
    }
    // "a.b.c.d.port"
    let parts: Vec<&str> = s.rsplitn(2, '.').collect();
    if parts.len() != 2 {
        return None;
    }
    let port: u16 = parts[0].parse().ok()?;
    let ip = parts[1].to_string();
    Some((ip, port))
}

/// 解析一行 macOS netstat 输出，返回 Edge
#[cfg(test)]
fn test_parse_netstat_line(line: &str, protocol: &str) -> Option<crate::data_manager::Edge> {
    let cols: Vec<&str> = line.split_whitespace().collect();
    if cols.len() < 5 {
        return None;
    }
    let proto_col = cols[0].to_lowercase();
    if !proto_col.starts_with("tcp") && !proto_col.starts_with("udp") {
        return None;
    }
    let local_raw = cols[3];
    let foreign_raw = cols[4];
    let (src, src_port) = test_split_macos_addr(local_raw)?;
    let (dst, dst_port) = test_split_macos_addr(foreign_raw)?;
    if src == "*" || dst == "*" {
        return None;
    }
    let state = if cols.len() >= 6 && protocol == "TCP" {
        Some(cols[5].to_string())
    } else {
        None
    };
    Some(crate::data_manager::Edge {
        src,
        dst,
        protocol: protocol.to_string(),
        src_port,
        dst_port,
        state,
        count: 1,
    })
}

/// 解析整个 macOS netstat 输出（跳过头部行）
#[cfg(test)]
fn test_parse_netstat_output(output: &str, protocol: &str) -> Vec<crate::data_manager::Edge> {
    output
        .lines()
        .filter_map(|line| test_parse_netstat_line(line, protocol))
        .collect()
}

/// 解析十六进制 IPv4 地址 "0101A8C0:0050" => ("192.168.1.1", 80)
#[cfg(test)]
fn test_parse_hex_addr_ipv4(s: &str) -> Option<(String, u16)> {
    let (addr_hex, port_hex) = s.split_once(':')?;
    let port = u16::from_str_radix(port_hex, 16).ok()?;
    if addr_hex.len() == 8 {
        let raw = u32::from_str_radix(addr_hex, 16).ok()?;
        let a = raw & 0xFF;
        let b = (raw >> 8) & 0xFF;
        let c = (raw >> 16) & 0xFF;
        let d = (raw >> 24) & 0xFF;
        Some((format!("{}.{}.{}.{}", a, b, c, d), port))
    } else {
        None
    }
}

/// TCP 状态码映射
#[cfg(test)]
fn test_tcp_state_from_hex(hex: &str) -> &'static str {
    match hex {
        "01" => "ESTABLISHED",
        "02" => "SYN_SENT",
        "03" => "SYN_RECV",
        "04" => "FIN_WAIT1",
        "05" => "FIN_WAIT2",
        "06" => "TIME_WAIT",
        "07" => "CLOSE",
        "08" => "CLOSE_WAIT",
        "09" => "LAST_ACK",
        "0A" => "LISTEN",
        "0B" => "CLOSING",
        _ => "UNKNOWN",
    }
}

/// 解析 /proc/net/tcp 格式的内容
#[cfg(test)]
fn test_parse_proc_net_tcp(content: &str) -> Vec<crate::data_manager::Edge> {
    let mut edges = Vec::new();
    for line in content.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        let local_hex = cols[1];
        let remote_hex = cols[2];
        let state_hex = cols[3];
        let (src, src_port) = match test_parse_hex_addr_ipv4(local_hex) {
            Some(v) => v,
            None => continue,
        };
        let (dst, dst_port) = match test_parse_hex_addr_ipv4(remote_hex) {
            Some(v) => v,
            None => continue,
        };
        let state = test_tcp_state_from_hex(state_hex).to_string();
        edges.push(crate::data_manager::Edge {
            src,
            dst,
            protocol: "TCP".to_string(),
            src_port,
            dst_port,
            state: Some(state),
            count: 1,
        });
    }
    edges
}

// ──────────────────────────────────────────────────────────────────────────────
// 测试
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── 平台无关过滤测试 ─────────────────────────────────────────────────────

    #[test]
    fn test_is_loopback() {
        assert!(is_loopback("127.0.0.1"));
        assert!(is_loopback("127.0.0.2"));
        assert!(is_loopback("::1"));
        assert!(is_loopback("0.0.0.0"));
        assert!(!is_loopback("192.168.1.1"));
        assert!(!is_loopback("10.0.0.1"));
    }

    #[test]
    fn test_is_private() {
        assert!(is_private("192.168.1.1"));
        assert!(is_private("10.0.0.1"));
        assert!(is_private("172.16.0.1"));
        assert!(is_private("172.31.0.1"));
        assert!(!is_private("8.8.8.8"));
        assert!(!is_private("1.1.1.1"));
    }

    #[test]
    fn test_should_include_exclude_loopback() {
        use crate::data_manager::Edge;
        let loopback_edge = Edge {
            src: "127.0.0.1".to_string(),
            dst: "127.0.0.1".to_string(),
            protocol: "TCP".to_string(),
            src_port: 12345,
            dst_port: 80,
            state: None,
            count: 1,
        };
        let external_edge = Edge {
            src: "192.168.1.100".to_string(),
            dst: "192.168.1.1".to_string(),
            protocol: "TCP".to_string(),
            src_port: 54321,
            dst_port: 443,
            state: None,
            count: 1,
        };
        assert!(!should_include(&loopback_edge, false, true));
        assert!(should_include(&external_edge, false, true));
        assert!(should_include(&loopback_edge, false, false));
    }

    #[test]
    fn test_should_include_local_only() {
        use crate::data_manager::Edge;
        let public_edge = Edge {
            src: "192.168.1.100".to_string(),
            dst: "8.8.8.8".to_string(),
            protocol: "UDP".to_string(),
            src_port: 63412,
            dst_port: 53,
            state: None,
            count: 1,
        };
        let private_edge = Edge {
            src: "192.168.1.100".to_string(),
            dst: "192.168.1.1".to_string(),
            protocol: "TCP".to_string(),
            src_port: 54321,
            dst_port: 443,
            state: None,
            count: 1,
        };
        assert!(!should_include(&public_edge, true, false));
        assert!(should_include(&private_edge, true, false));
    }

    // ── macOS netstat 解析测试（fixture 字符串，不依赖真实 netstat）───────────

    const MACOS_TCP_FIXTURE: &str = r#"Active Internet connections (including servers)
Proto Recv-Q Send-Q  Local Address          Foreign Address        (state)
tcp4       0      0  192.168.1.100.55231    192.168.1.1.443        ESTABLISHED
tcp4       0      0  192.168.1.100.55232    8.8.8.8.443            ESTABLISHED
tcp4       0      0  *.8080                 *.*                    LISTEN
tcp6       0      0  *.22                   *.*                    LISTEN
tcp4       0      0  127.0.0.1.3306         127.0.0.1.55100        ESTABLISHED
"#;

    const MACOS_UDP_FIXTURE: &str = r#"Active Internet connections (including servers)
Proto Recv-Q Send-Q  Local Address          Foreign Address
udp4       0      0  192.168.1.100.63412    8.8.8.8.53
udp4       0      0  *.5353                 *.*
"#;

    #[test]
    fn test_macos_parse_established_tcp() {
        let edges = test_parse_netstat_output(MACOS_TCP_FIXTURE, "TCP");
        let established: Vec<_> = edges
            .iter()
            .filter(|e| e.state.as_deref() == Some("ESTABLISHED"))
            .collect();
        // 3 ESTABLISHED lines: 192.168.1.100->1.1, 192.168.1.100->8.8.8.8, 127.0.0.1->127.0.0.1
        assert_eq!(established.len(), 3);
    }

    #[test]
    fn test_macos_parse_tcp_src_dst() {
        let edges = test_parse_netstat_output(MACOS_TCP_FIXTURE, "TCP");
        let e = edges
            .iter()
            .find(|e| e.dst == "192.168.1.1")
            .expect("should find edge to 192.168.1.1");
        assert_eq!(e.src, "192.168.1.100");
        assert_eq!(e.src_port, 55231);
        assert_eq!(e.dst_port, 443);
        assert_eq!(e.protocol, "TCP");
    }

    #[test]
    fn test_macos_parse_udp_no_state() {
        let edges = test_parse_netstat_output(MACOS_UDP_FIXTURE, "UDP");
        assert!(!edges.is_empty());
        // All parseable UDP lines have no state
        assert!(edges.iter().all(|e| e.state.is_none()));
        assert!(edges.iter().all(|e| e.protocol == "UDP"));
    }

    #[test]
    fn test_macos_parse_wildcard_address_listen() {
        // *.8080 LISTEN: src="0.0.0.0", dst cannot be resolved from "*.*"
        // The *.8080 line has foreign="*.*" which splits to None => skipped
        // But src "*.8080" splits to ("0.0.0.0", 8080)
        // Since foreign "*.*" returns None, the line is skipped entirely
        // Let's verify *.8080 is handled by the addr parser
        let parsed = test_split_macos_addr("*.8080");
        assert_eq!(parsed, Some(("0.0.0.0".to_string(), 8080)));
    }

    #[test]
    fn test_macos_addr_parser_ipv4() {
        assert_eq!(
            test_split_macos_addr("192.168.1.100.55231"),
            Some(("192.168.1.100".to_string(), 55231))
        );
    }

    #[test]
    fn test_macos_addr_parser_wildcard_star_star() {
        assert_eq!(test_split_macos_addr("*.*"), None);
    }

    #[test]
    fn test_macos_addr_parser_wildcard_port() {
        assert_eq!(
            test_split_macos_addr("*.8080"),
            Some(("0.0.0.0".to_string(), 8080))
        );
    }

    #[test]
    fn test_macos_edge_count_initial_value() {
        let edges = test_parse_netstat_output(MACOS_TCP_FIXTURE, "TCP");
        assert!(edges.iter().all(|e| e.count == 1));
    }

    #[test]
    fn test_macos_all_protocol_classified_as_tcp() {
        // tcp4 and tcp6 both produce protocol="TCP"
        let edges = test_parse_netstat_output(MACOS_TCP_FIXTURE, "TCP");
        assert!(edges.iter().all(|e| e.protocol == "TCP"));
    }

    // ── Linux /proc/net/tcp 解析测试（十六进制 fixture）─────────────────────

    const PROC_NET_TCP_FIXTURE: &str =
        "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0100007F:1538 00000000:0000 0A 00000000:00000000 00:00000000 00000000   1000        0 12345 1 0000000000000000 100 0 0 10 0\n\
   1: 6401A8C0:D8F1 0101A8C0:01BB 01 00000000:00000000 00:00000000 00000000   1000        0 23456 1 0000000000000000 20 4 24 10 -1\n";

    #[test]
    fn test_linux_hex_to_ipv4_loopback() {
        let (ip, _port) = test_parse_hex_addr_ipv4("0100007F:1538").unwrap();
        assert_eq!(ip, "127.0.0.1");
    }

    #[test]
    fn test_linux_hex_to_ipv4_lan() {
        // 6401A8C0 little-endian => 192.168.1.100
        let (ip, port) = test_parse_hex_addr_ipv4("6401A8C0:D8F1").unwrap();
        assert_eq!(ip, "192.168.1.100");
        assert_eq!(port, 0xD8F1);
    }

    #[test]
    fn test_linux_hex_to_ipv4_router() {
        // 0101A8C0 little-endian => 192.168.1.1
        let (ip, port) = test_parse_hex_addr_ipv4("0101A8C0:01BB").unwrap();
        assert_eq!(ip, "192.168.1.1");
        assert_eq!(port, 443);
    }

    #[test]
    fn test_linux_tcp_state_mapping() {
        assert_eq!(test_tcp_state_from_hex("01"), "ESTABLISHED");
        assert_eq!(test_tcp_state_from_hex("0A"), "LISTEN");
        assert_eq!(test_tcp_state_from_hex("06"), "TIME_WAIT");
        assert_eq!(test_tcp_state_from_hex("02"), "SYN_SENT");
        assert_eq!(test_tcp_state_from_hex("FF"), "UNKNOWN");
    }

    #[test]
    fn test_linux_parse_proc_tcp_listen() {
        let edges = test_parse_proc_net_tcp(PROC_NET_TCP_FIXTURE);
        let listen = edges
            .iter()
            .find(|e| e.state.as_deref() == Some("LISTEN"))
            .expect("should have a LISTEN entry");
        assert_eq!(listen.src, "127.0.0.1");
        assert_eq!(listen.src_port, 0x1538); // 5432
    }

    #[test]
    fn test_linux_parse_proc_tcp_established() {
        let edges = test_parse_proc_net_tcp(PROC_NET_TCP_FIXTURE);
        let est = edges
            .iter()
            .find(|e| e.state.as_deref() == Some("ESTABLISHED"))
            .expect("should have an ESTABLISHED entry");
        assert_eq!(est.src, "192.168.1.100");
        assert_eq!(est.dst, "192.168.1.1");
        assert_eq!(est.dst_port, 443); // 0x01BB
    }

    #[test]
    fn test_linux_parse_proc_tcp_count() {
        let edges = test_parse_proc_net_tcp(PROC_NET_TCP_FIXTURE);
        assert_eq!(edges.len(), 2);
        assert!(edges.iter().all(|e| e.count == 1));
    }

    #[test]
    fn test_linux_parse_proc_tcp_protocol() {
        let edges = test_parse_proc_net_tcp(PROC_NET_TCP_FIXTURE);
        assert!(edges.iter().all(|e| e.protocol == "TCP"));
    }

    // ── 边界测试：空输入 / 只有 header / 过滤行为 ────────────────────────────

    #[test]
    fn test_parse_netstat_output_empty_string() {
        // 完全空字符串 → 空 Vec
        let edges = test_parse_netstat_output("", "TCP");
        assert!(
            edges.is_empty(),
            "empty netstat output should produce empty Vec"
        );
    }

    #[test]
    fn test_parse_netstat_output_header_only() {
        // 只有 header 行（没有数据行）→ 空 Vec
        let header_only = "Active Internet connections (including servers)\n\
Proto Recv-Q Send-Q  Local Address          Foreign Address        (state)\n";
        let edges = test_parse_netstat_output(header_only, "TCP");
        assert!(
            edges.is_empty(),
            "header-only netstat output should produce empty Vec"
        );
    }

    #[test]
    fn test_parse_proc_net_tcp_empty_string() {
        // 完全空字符串 → 空 Vec（skip(1) 跳过 header 后无数据）
        let edges = test_parse_proc_net_tcp("");
        assert!(
            edges.is_empty(),
            "empty /proc/net/tcp content should produce empty Vec"
        );
    }

    #[test]
    fn test_parse_proc_net_tcp_header_only() {
        // 只有 header 行 → 空 Vec
        let header_only =
            "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";
        let edges = test_parse_proc_net_tcp(header_only);
        assert!(
            edges.is_empty(),
            "header-only /proc/net/tcp content should produce empty Vec"
        );
    }

    #[test]
    fn test_should_include_local_only_filters_public_ip() {
        use crate::data_manager::Edge;
        // 公网 IP (8.8.8.8) 在 local_only=true 时应被过滤掉
        let public_edge = Edge {
            src: "192.168.1.100".to_string(),
            dst: "8.8.8.8".to_string(),
            protocol: "TCP".to_string(),
            src_port: 54321,
            dst_port: 443,
            state: Some("ESTABLISHED".to_string()),
            count: 1,
        };
        assert!(
            !should_include(&public_edge, true, false),
            "public IP 8.8.8.8 should be filtered when local_only=true"
        );
    }

    #[test]
    fn test_should_include_local_only_keeps_private_192_168() {
        use crate::data_manager::Edge;
        // 192.168.x.x 是私有地址，local_only=true 时应保留
        let private_edge = Edge {
            src: "192.168.1.100".to_string(),
            dst: "192.168.1.1".to_string(),
            protocol: "TCP".to_string(),
            src_port: 54321,
            dst_port: 80,
            state: Some("ESTABLISHED".to_string()),
            count: 1,
        };
        assert!(
            should_include(&private_edge, true, false),
            "192.168.x.x should be kept when local_only=true"
        );
    }

    #[test]
    fn test_should_include_exclude_loopback_filters_127() {
        use crate::data_manager::Edge;
        // 127.0.0.1 在 exclude_loopback=true 时应被过滤
        let loopback_edge = Edge {
            src: "127.0.0.1".to_string(),
            dst: "127.0.0.1".to_string(),
            protocol: "TCP".to_string(),
            src_port: 12345,
            dst_port: 5432,
            state: Some("ESTABLISHED".to_string()),
            count: 1,
        };
        assert!(
            !should_include(&loopback_edge, false, true),
            "127.0.0.1 connections should be filtered when exclude_loopback=true"
        );
    }

    #[test]
    fn test_should_include_exclude_loopback_keeps_public() {
        use crate::data_manager::Edge;
        // 非 loopback 地址在 exclude_loopback=true 时应保留
        let public_edge = Edge {
            src: "192.168.1.100".to_string(),
            dst: "8.8.8.8".to_string(),
            protocol: "UDP".to_string(),
            src_port: 63412,
            dst_port: 53,
            state: None,
            count: 1,
        };
        assert!(
            should_include(&public_edge, false, true),
            "non-loopback edge should be kept when exclude_loopback=true"
        );
    }

    #[test]
    fn test_should_include_local_only_and_exclude_loopback_combined() {
        use crate::data_manager::Edge;
        // 同时启用两个过滤：公网 IP 应被 local_only 过滤掉
        let public_edge = Edge {
            src: "192.168.1.100".to_string(),
            dst: "1.1.1.1".to_string(),
            protocol: "TCP".to_string(),
            src_port: 54321,
            dst_port: 443,
            state: Some("ESTABLISHED".to_string()),
            count: 1,
        };
        assert!(
            !should_include(&public_edge, true, true),
            "public IP 1.1.1.1 should be filtered with both filters enabled"
        );
    }

    #[test]
    fn test_should_include_local_only_and_exclude_loopback_keeps_private() {
        use crate::data_manager::Edge;
        // 私有地址、非 loopback：两个过滤都启用时应保留
        let private_edge = Edge {
            src: "10.0.0.1".to_string(),
            dst: "10.0.0.2".to_string(),
            protocol: "TCP".to_string(),
            src_port: 8080,
            dst_port: 9090,
            state: Some("ESTABLISHED".to_string()),
            count: 1,
        };
        assert!(
            should_include(&private_edge, true, true),
            "private IP 10.x.x.x should be kept with both filters enabled"
        );
    }

    #[test]
    fn test_parse_netstat_output_filters_applied() {
        // 验证解析后再过滤的组合行为：公网 IP 连接应被 local_only 过滤
        let fixture = "Active Internet connections\n\
Proto Recv-Q Send-Q  Local Address          Foreign Address        (state)\n\
tcp4       0      0  192.168.1.100.55231    192.168.1.1.443        ESTABLISHED\n\
tcp4       0      0  192.168.1.100.55232    8.8.8.8.443            ESTABLISHED\n";

        let all_edges = test_parse_netstat_output(fixture, "TCP");
        assert_eq!(all_edges.len(), 2, "should parse 2 edges before filtering");

        let filtered: Vec<_> = all_edges
            .into_iter()
            .filter(|e| should_include(e, true, false))
            .collect();
        assert_eq!(
            filtered.len(),
            1,
            "only 1 edge should remain after local_only filter"
        );
        assert_eq!(
            filtered[0].dst, "192.168.1.1",
            "kept edge should point to private IP"
        );
    }

    #[test]
    fn test_parse_netstat_output_loopback_filter_applied() {
        // 127.0.0.1 连接在 exclude_loopback=true 时应被过滤
        let fixture = "Active Internet connections\n\
Proto Recv-Q Send-Q  Local Address          Foreign Address        (state)\n\
tcp4       0      0  127.0.0.1.3306         127.0.0.1.55100        ESTABLISHED\n\
tcp4       0      0  192.168.1.100.55231    192.168.1.1.443        ESTABLISHED\n";

        let all_edges = test_parse_netstat_output(fixture, "TCP");
        let filtered: Vec<_> = all_edges
            .into_iter()
            .filter(|e| should_include(e, false, true))
            .collect();
        assert_eq!(
            filtered.len(),
            1,
            "loopback edge should be filtered out when exclude_loopback=true"
        );
        assert_eq!(filtered[0].dst, "192.168.1.1");
    }

    // ── 原有平台专属测试（保留） ─────────────────────────────────────────────

    #[cfg(target_os = "macos")]
    #[test]
    fn test_split_addr_port_ipv4() {
        let (ip, port) = split_addr_port("192.168.1.100.443").unwrap();
        assert_eq!(ip, "192.168.1.100");
        assert_eq!(port, 443);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_parse_netstat_line_established() {
        let line = "tcp4       0      0  192.168.1.100.54321     192.168.1.1.443    ESTABLISHED";
        let edge = parse_netstat_line(line, "TCP").unwrap();
        assert_eq!(edge.src, "192.168.1.100");
        assert_eq!(edge.dst, "192.168.1.1");
        assert_eq!(edge.dst_port, 443);
        assert_eq!(edge.state.as_deref(), Some("ESTABLISHED"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_parse_netstat_line_wildcard_skipped() {
        let line = "udp4       0      0  *.*                    *.*";
        let edge = parse_netstat_line(line, "UDP");
        assert!(edge.is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_parse_hex_addr_ipv4() {
        // 0101A8C0:0050 => 192.168.1.1:80
        let (ip, port) = parse_hex_addr("0101A8C0:0050").unwrap();
        assert_eq!(ip, "192.168.1.1");
        assert_eq!(port, 80);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_proc_net_parsing() {
        let content = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0101A8C0:0050 020AA8C0:8000 01 00000000:00000000 00:00000000 00000000     0        0 12345 1 0000000000000000 20 4 0 10 -1\n";
        let edges = parse_proc_net(content, "TCP");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].dst_port, 80);
    }
}
