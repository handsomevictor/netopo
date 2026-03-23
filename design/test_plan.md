# netopo — 测试计划

> 本文档依据 CLAUDE.md 第八节（STOP 条件 / 测试覆盖要求）编写。
> 目标：每个模块 line coverage >= 70%；TUI 测试使用 TestBackend；
> connection_tracker 提供 macOS + Linux 两套解析的单元测试。

---

## 一、测试策略总览

| 模块 | 测试类型 | 关键约束 |
|------|---------|---------|
| data_manager.rs | 单元测试 | serde 序列化/反序列化往返一致 |
| cli.rs | 单元测试 | 参数解析正确，默认值符合规格 |
| scanner.rs | 单元测试 + 集成测试 | 端口解析纯逻辑可单测；子网扫描需真实网络（集成） |
| connection_tracker.rs | 单元测试（重点） | macOS 和 Linux 各一套解析 fixture |
| graph_builder.rs | 单元测试 | 去重、合并、过滤逻辑全覆盖 |
| visualization.rs | 单元测试 + TUI 专项 | TUI 使用 ratatui::backend::TestBackend |

---

## 二、data_manager.rs 测试

### 2.1 测试目标

验证 CLAUDE.md 第四节定义的四个结构体（`Node`、`Edge`、`Graph`、`GraphSummary`）的 serde 实现正确。

### 2.2 测试用例

```rust
// tests/data_manager_tests.rs 或 src/data_manager.rs 内 #[cfg(test)]

#[test]
fn test_node_serde_roundtrip() {
    let node = Node {
        ip: "192.168.1.1".to_string(),
        hostname: Some("router.local".to_string()),
        ports: vec![80, 443],
        is_local: false,
        mac: Some("aa:bb:cc:dd:ee:ff".to_string()),
        interface: None,
    };
    let json = serde_json::to_string(&node).unwrap();
    let decoded: Node = serde_json::from_str(&json).unwrap();
    assert_eq!(node.ip, decoded.ip);
    assert_eq!(node.ports, decoded.ports);
}

#[test]
fn test_edge_protocol_values() {
    // protocol 字段只能是 "TCP" 或 "UDP"
    let edge = Edge {
        src: "192.168.1.100".to_string(),
        dst: "192.168.1.1".to_string(),
        protocol: "TCP".to_string(),
        src_port: 54321,
        dst_port: 443,
        state: Some("ESTABLISHED".to_string()),
        count: 1,
    };
    let json = serde_json::to_string(&edge).unwrap();
    assert!(json.contains("\"TCP\""));
}

#[test]
fn test_graph_captured_at_rfc3339() {
    // captured_at 必须是合法的 RFC3339 格式
    let graph = Graph { /* ... */ captured_at: "2024-01-15T10:30:00+08:00".to_string(), /* ... */ };
    assert!(chrono::DateTime::parse_from_rfc3339(&graph.captured_at).is_ok());
}

#[test]
fn test_graph_summary_consistency() {
    // summary 中的计数必须与 nodes/edges 实际数量一致
    // 由 graph_builder 保证，这里只验证序列化字段存在
}
```

覆盖率目标：>= 80%（结构体本身逻辑简单，主要通过 serde 测试覆盖）。

---

## 三、cli.rs 测试

### 3.1 测试目标

验证 `clap` 参数解析行为：默认值、必填项、类型转换。

### 3.2 测试用例

```rust
#[test]
fn test_default_values() {
    let args = CliArgs::try_parse_from(["netopo"]).unwrap();
    assert_eq!(args.concurrency, 256);
    assert_eq!(args.timeout, 500);
    assert!(!args.scan);
    assert!(!args.tui);
}

#[test]
fn test_scan_with_subnet() {
    let args = CliArgs::try_parse_from(["netopo", "--scan", "--subnet", "10.0.0.0/24"]).unwrap();
    assert!(args.scan);
    assert_eq!(args.subnet.as_deref(), Some("10.0.0.0/24"));
}

#[test]
fn test_output_flags_exclusive_paths() {
    // --output-json 和 --output-dot 可同时使用（非互斥）
    let args = CliArgs::try_parse_from([
        "netopo", "--output-json", "out.json", "--output-dot", "out.dot"
    ]).unwrap();
    assert!(args.output_json.is_some());
    assert!(args.output_dot.is_some());
}

#[test]
fn test_watch_seconds_parsed() {
    let args = CliArgs::try_parse_from(["netopo", "--connections", "--watch", "10"]).unwrap();
    assert_eq!(args.watch, Some(10));
}
```

---

## 四、scanner.rs 测试

### 4.1 端口字符串解析（纯逻辑，无 I/O）

```rust
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
fn test_parse_ports_invalid_returns_error() {
    assert!(parse_ports("abc").is_err());
    assert!(parse_ports("99999").is_err()); // 超出 u16 范围
}
```

### 4.2 子网 IP 枚举（纯逻辑）

```rust
#[test]
fn test_cidr_to_host_ips_slash24() {
    // 192.168.1.0/24 应枚举 192.168.1.1 ~ 192.168.1.254（254 个主机地址）
    let ips = cidr_to_host_ips("192.168.1.0/24").unwrap();
    assert_eq!(ips.len(), 254);
    assert!(ips.contains(&"192.168.1.1".parse().unwrap()));
    assert!(!ips.contains(&"192.168.1.0".parse().unwrap())); // 网络地址排除
    assert!(!ips.contains(&"192.168.1.255".parse().unwrap())); // 广播地址排除
}

#[test]
fn test_cidr_invalid_returns_error() {
    assert!(cidr_to_host_ips("not-a-cidr").is_err());
}
```

### 4.3 集成测试（需要真实网络，标记 #[ignore]）

```rust
#[tokio::test]
#[ignore] // 需要真实网络环境，CI 中跳过
async fn test_scan_loopback() {
    // 扫描 127.0.0.1/32，至少本机 loopback 应该有响应
    let nodes = scan_subnet("127.0.0.1/32", &[22, 80], 10, 500).await.unwrap();
    // loopback 的响应取决于实际开放端口，只验证不报错
    assert!(nodes.len() <= 1);
}
```

---

## 五、connection_tracker.rs 测试（重点）

CLAUDE.md 第八节明确要求："connection_tracker 有 Linux/macOS 两套解析的单元测试"。

所有解析测试使用 **fixture 字符串**，不依赖真实 `netstat` 或 `/proc`。

### 5.1 macOS netstat 解析（当前平台，主要路径）

```rust
#[cfg(test)]
mod macos_tests {
    use super::*;

    // Fixture：macOS netstat -an -p tcp 真实输出样本
    const MACOS_TCP_OUTPUT: &str = r#"Active Internet connections (including servers)
Proto Recv-Q Send-Q  Local Address          Foreign Address        (state)
tcp4       0      0  192.168.1.100.55231    192.168.1.1.443        ESTABLISHED
tcp4       0      0  192.168.1.100.55232    8.8.8.8.443            ESTABLISHED
tcp4       0      0  *.8080                 *.*                    LISTEN
tcp6       0      0  *.22                   *.*                    LISTEN
tcp4       0      0  127.0.0.1.3306         127.0.0.1.55100        ESTABLISHED
"#;

    const MACOS_UDP_OUTPUT: &str = r#"Active Internet connections (including servers)
Proto Recv-Q Send-Q  Local Address          Foreign Address
udp4       0      0  192.168.1.100.63412    8.8.8.8.53
udp4       0      0  *.5353                 *.*
"#;

    #[test]
    fn test_parse_established_tcp() {
        let edges = parse_netstat_output(MACOS_TCP_OUTPUT, "TCP").unwrap();
        // 应解析出 3 条 ESTABLISHED 连接（排除 LISTEN）
        let established: Vec<_> = edges.iter()
            .filter(|e| e.state.as_deref() == Some("ESTABLISHED"))
            .collect();
        assert_eq!(established.len(), 3);
    }

    #[test]
    fn test_parse_tcp_src_dst() {
        let edges = parse_netstat_output(MACOS_TCP_OUTPUT, "TCP").unwrap();
        let first = edges.iter()
            .find(|e| e.dst == "192.168.1.1")
            .unwrap();
        assert_eq!(first.src, "192.168.1.100");
        assert_eq!(first.src_port, 55231);
        assert_eq!(first.dst_port, 443);
        assert_eq!(first.protocol, "TCP");
    }

    #[test]
    fn test_parse_udp_no_state() {
        let edges = parse_netstat_output(MACOS_UDP_OUTPUT, "UDP").unwrap();
        assert!(!edges.is_empty());
        assert!(edges.iter().all(|e| e.state.is_none()));
        assert!(edges.iter().all(|e| e.protocol == "UDP"));
    }

    #[test]
    fn test_parse_wildcard_address() {
        let edges = parse_netstat_output(MACOS_TCP_OUTPUT, "TCP").unwrap();
        // *.8080 应解析为 ip="0.0.0.0" port=8080
        let listen = edges.iter().find(|e| e.src_port == 8080);
        if let Some(e) = listen {
            assert_eq!(e.src, "0.0.0.0");
        }
    }

    #[test]
    fn test_parse_tcp4_and_tcp6_both_classified_as_tcp() {
        let edges = parse_netstat_output(MACOS_TCP_OUTPUT, "TCP").unwrap();
        // tcp4 和 tcp6 都应归类为 "TCP"
        assert!(edges.iter().all(|e| e.protocol == "TCP"));
    }

    #[test]
    fn test_macos_addr_parser() {
        assert_eq!(
            parse_macos_addr("192.168.1.100.55231"),
            Some(("192.168.1.100".to_string(), 55231))
        );
        assert_eq!(
            parse_macos_addr("*.8080"),
            Some(("0.0.0.0".to_string(), 8080))
        );
        assert_eq!(
            parse_macos_addr("*.*"),
            None // 无法解析端口
        );
    }

    #[test]
    fn test_edge_count_initial_value() {
        let edges = parse_netstat_output(MACOS_TCP_OUTPUT, "TCP").unwrap();
        // snapshot 返回的 Edge.count 初始值必须为 1（合并由 graph_builder 负责）
        assert!(edges.iter().all(|e| e.count == 1));
    }
}
```

### 5.2 Linux /proc/net/tcp 解析

```rust
#[cfg(test)]
mod linux_tests {
    use super::*;

    // Fixture：/proc/net/tcp 真实行样本（十六进制小端序）
    const PROC_NET_TCP_LINES: &str = r#"  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1538 00000000:0000 0A 00000000:00000000 00:00000000 00000000   1000        0 12345 1 0000000000000000 100 0 0 10 0
   1: 6401A8C0:D8F1 0101A8C0:01BB 01 00000000:00000000 00:00000000 00000000   1000        0 23456 1 0000000000000000 20 4 24 10 -1
"#;

    #[test]
    fn test_parse_proc_tcp_listen() {
        let edges = parse_proc_net_tcp(PROC_NET_TCP_LINES).unwrap();
        // st=0A 表示 LISTEN
        let listen = edges.iter().find(|e| e.state.as_deref() == Some("LISTEN"));
        assert!(listen.is_some());
        let listen = listen.unwrap();
        assert_eq!(listen.src, "127.0.0.1");
        assert_eq!(listen.src_port, 0x1538); // 5432（PostgreSQL）
    }

    #[test]
    fn test_parse_proc_tcp_established() {
        let edges = parse_proc_net_tcp(PROC_NET_TCP_LINES).unwrap();
        // st=01 表示 ESTABLISHED
        let est = edges.iter().find(|e| e.state.as_deref() == Some("ESTABLISHED"));
        assert!(est.is_some());
        let est = est.unwrap();
        // 6401A8C0 小端序 = 192.168.1.100
        assert_eq!(est.src, "192.168.1.100");
        // 0101A8C0 = 192.168.1.1
        assert_eq!(est.dst, "192.168.1.1");
        assert_eq!(est.dst_port, 443); // 0x01BB
    }

    #[test]
    fn test_hex_to_ipv4() {
        // 辅助函数测试
        assert_eq!(hex_to_ipv4("0100007F").unwrap(), "127.0.0.1");
        assert_eq!(hex_to_ipv4("6401A8C0").unwrap(), "192.168.1.100");
        assert_eq!(hex_to_ipv4("0101A8C0").unwrap(), "192.168.1.1");
    }

    #[test]
    fn test_tcp_state_mapping() {
        assert_eq!(tcp_state_from_hex("01"), "ESTABLISHED");
        assert_eq!(tcp_state_from_hex("0A"), "LISTEN");
        assert_eq!(tcp_state_from_hex("06"), "TIME_WAIT");
    }
}
```

### 5.3 过滤选项测试

```rust
#[test]
fn test_filter_loopback() {
    let edges = vec![
        Edge { src: "127.0.0.1".to_string(), dst: "127.0.0.1".to_string(), /* ... */ count: 1, .. },
        Edge { src: "192.168.1.100".to_string(), dst: "192.168.1.1".to_string(), /* ... */ count: 1, .. },
    ];
    let filtered = filter_edges(edges, &SnapshotOptions { exclude_loopback: true, local_only: false });
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].src, "192.168.1.100");
}
```

---

## 六、graph_builder.rs 测试

### 6.1 节点去重与合并

```rust
#[test]
fn test_node_dedup_same_ip() {
    let nodes = vec![
        Node { ip: "192.168.1.1".to_string(), ports: vec![80], is_local: false, /* ... */ },
        Node { ip: "192.168.1.1".to_string(), ports: vec![443], is_local: false, /* ... */ },
    ];
    let graph = build(nodes, vec![]).unwrap();
    assert_eq!(graph.nodes.len(), 1);
    assert!(graph.nodes[0].ports.contains(&80));
    assert!(graph.nodes[0].ports.contains(&443));
}
```

### 6.2 Edge 去重与 count 累加

```rust
#[test]
fn test_edge_dedup_and_count() {
    let edges = vec![
        Edge { src: "192.168.1.100".to_string(), dst: "192.168.1.1".to_string(),
               protocol: "TCP".to_string(), src_port: 12345, dst_port: 443,
               state: Some("ESTABLISHED".to_string()), count: 1 },
        Edge { src: "192.168.1.100".to_string(), dst: "192.168.1.1".to_string(),
               protocol: "TCP".to_string(), src_port: 12346, dst_port: 443,
               state: Some("ESTABLISHED".to_string()), count: 1 },
    ];
    let graph = build(vec![], edges).unwrap();
    // 相同 (src, dst, protocol, dst_port) 合并为一条边
    let merged: Vec<_> = graph.edges.iter()
        .filter(|e| e.dst_port == 443)
        .collect();
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].count, 2);
}
```

### 6.3 GraphSummary 正确性

```rust
#[test]
fn test_graph_summary_counts() {
    let nodes = vec![
        Node { ip: "192.168.1.100".to_string(), is_local: true, ports: vec![], /* ... */ },
        Node { ip: "192.168.1.1".to_string(), is_local: false, ports: vec![], /* ... */ },
    ];
    let edges = vec![
        Edge { protocol: "TCP".to_string(), count: 1, /* ... */ },
        Edge { protocol: "UDP".to_string(), count: 1, /* ... */ },
    ];
    let graph = build(nodes, edges).unwrap();
    assert_eq!(graph.summary.total_nodes, 2);
    assert_eq!(graph.summary.total_edges, 2);
    assert_eq!(graph.summary.tcp_connections, 1);
    assert_eq!(graph.summary.udp_connections, 1);
}
```

### 6.4 min_connections 过滤

```rust
#[test]
fn test_filter_min_connections() {
    // 创建节点 A 有 3 条连接，节点 B 只有 1 条
    // filter_by_min_connections(graph, 2) 后 B 应被移除
}
```

### 6.5 inbound/outbound 查询

```rust
#[test]
fn test_inbound_outbound_edges() {
    // 验证 inbound_edges("192.168.1.1") 返回目标是 192.168.1.1 的边
    // 验证 outbound_edges("192.168.1.100") 返回源是 192.168.1.100 的边
}
```

---

## 七、visualization.rs 测试

### 7.1 F5 JSON 输出

```rust
#[test]
fn test_write_json_valid_schema() {
    let graph = make_test_graph();
    let tmp = tempfile::NamedTempFile::new().unwrap();
    write_json(&graph, tmp.path()).unwrap();
    let content = std::fs::read_to_string(tmp.path()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    // 验证顶层字段存在
    assert!(parsed.get("nodes").is_some());
    assert!(parsed.get("edges").is_some());
    assert!(parsed.get("captured_at").is_some());
    assert!(parsed.get("local_ip").is_some());
    assert!(parsed.get("summary").is_some());
}
```

### 7.2 F6 Dot 输出

```rust
#[test]
fn test_write_dot_contains_required_elements() {
    let graph = make_test_graph(); // local_ip = "192.168.1.100"
    let tmp = tempfile::NamedTempFile::new().unwrap();
    write_dot(&graph, tmp.path()).unwrap();
    let content = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(content.contains("digraph netopo"));
    assert!(content.contains("rankdir=LR"));
    assert!(content.contains("doublecircle")); // 本机节点
    assert!(content.contains("color=blue"));   // 本机节点颜色
}

#[test]
fn test_dot_udp_dashed_style() {
    // UDP 边应包含 style=dashed
    let graph = make_test_graph_with_udp();
    let content = render_dot_string(&graph);
    assert!(content.contains("style=dashed"));
}
```

### 7.3 F7 ASCII 输出

```rust
#[test]
fn test_ascii_contains_local_star_marker() {
    let graph = make_test_graph(); // local_ip = "192.168.1.100"
    let output = render_ascii_string(&graph).unwrap();
    assert!(output.contains("★"));
    assert!(output.contains("192.168.1.100"));
}

#[test]
fn test_ascii_80_column_limit() {
    let graph = make_test_graph();
    let output = render_ascii_string(&graph).unwrap();
    for line in output.lines() {
        // 按字符计数（Unicode 宽字符需用 unicode-width）
        assert!(line.chars().count() <= 80,
            "Line exceeds 80 cols: {:?}", line);
    }
}

#[test]
fn test_ascii_tcp_udp_arrows() {
    let graph = make_test_graph_with_both_protocols();
    let output = render_ascii_string(&graph).unwrap();
    assert!(output.contains("TCP"));
    assert!(output.contains("UDP"));
}
```

### 7.4 F8 TUI 测试（TestBackend）

```rust
use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn test_tui_initial_render_no_panic() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    let graph = make_test_graph();
    let mut app = TuiApp::new(graph);

    terminal.draw(|f| app.render(f)).unwrap();
    // 只要不 panic 且能正常渲染即通过
}

#[test]
fn test_tui_four_panels_present() {
    let backend = TestBackend::new(120, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    let graph = make_test_graph();
    let mut app = TuiApp::new(graph);

    terminal.draw(|f| app.render(f)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let content: String = buffer.content().iter().map(|c| c.symbol()).collect();
    // 验证四个区域的标题文本存在
    assert!(content.contains("节点") || content.contains("Node"));
    assert!(content.contains("连接") || content.contains("Connection"));
}

#[test]
fn test_tui_key_q_sets_quit_flag() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let graph = make_test_graph();
    let mut app = TuiApp::new(graph);
    let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    app.handle_key(key);
    assert!(app.should_quit);
}

#[test]
fn test_tui_key_up_down_changes_selection() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    let graph = make_test_graph(); // 至少 3 个节点
    let mut app = TuiApp::new(graph);
    let initial = app.selected_index;
    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    assert_eq!(app.selected_index, initial + 1);
    app.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(app.selected_index, initial);
}
```

---

## 八、集成测试

位于 `tests/` 目录，测试跨模块完整流程。

```rust
// tests/integration_test.rs

#[tokio::test]
async fn test_full_pipeline_with_mock_data() {
    // 1. 构造测试数据（不调用真实网络）
    let nodes = vec![
        Node { ip: "192.168.1.100".to_string(), is_local: true, ports: vec![22, 80], /* ... */ },
        Node { ip: "192.168.1.1".to_string(), is_local: false, ports: vec![80, 443], /* ... */ },
    ];
    let edges = vec![
        Edge { src: "192.168.1.100".to_string(), dst: "192.168.1.1".to_string(),
               protocol: "TCP".to_string(), dst_port: 443, src_port: 55000,
               state: Some("ESTABLISHED".to_string()), count: 1 },
    ];

    // 2. 构建图
    let graph = graph_builder::build(nodes, edges).unwrap();
    assert_eq!(graph.summary.total_nodes, 2);

    // 3. JSON 序列化并验证
    let json = serde_json::to_string(&graph).unwrap();
    let reparsed: Graph = serde_json::from_str(&json).unwrap();
    assert_eq!(reparsed.local_ip, "192.168.1.100");

    // 4. dot 渲染不报错
    let tmp = tempfile::NamedTempFile::new().unwrap();
    visualization::write_dot(&graph, tmp.path()).unwrap();
}
```

---

## 九、覆盖率收集

使用 `cargo-llvm-cov` 收集 line coverage：

```bash
# 安装
cargo install cargo-llvm-cov

# 运行并输出报告
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info

# 查看摘要
cargo llvm-cov --all-features --workspace
```

目标阈值（CLAUDE.md 第八节）：每个模块 line coverage >= 70%。

CI 中可通过如下命令断言：

```bash
cargo llvm-cov --all-features --workspace --fail-under-lines 70
```

---

## 十、测试辅助函数（共享 fixture）

在 `tests/common/mod.rs` 或各模块 `#[cfg(test)]` 块中提供：

```rust
pub fn make_test_graph() -> Graph {
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
        edges: vec![
            Edge {
                src: "192.168.1.100".to_string(),
                dst: "192.168.1.1".to_string(),
                protocol: "TCP".to_string(),
                src_port: 55000,
                dst_port: 443,
                state: Some("ESTABLISHED".to_string()),
                count: 3,
            },
        ],
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
```

---

## 十一、Red-Green 测试协议（tester_agent 执行步骤）

依据 CLAUDE.md 第九节规定，tester_agent 不得跳过 Red 阶段：

1. **Red**：先运行 `cargo test`，确认新增测试用例初始状态为 FAIL（或编译失败）。
2. **Green**：developer_agent 实现对应功能后，再次运行 `cargo test`，确认全部 PASS。
3. **Refactor**：若代码有重复/不清晰，在测试绿灯下重构，确保测试仍然通过。
4. 每次提交前必须通过 `cargo test --all`、`cargo clippy -- -D warnings`、`cargo fmt --check`。
