//! graph_builder — F4: 使用 petgraph 构建有向图，去重合并连接

use crate::data_manager::{Edge, Graph, GraphSummary, Node};
use chrono::Utc;
use petgraph::graph::DiGraph;
use std::collections::HashMap;
use std::time::Instant;

/// 构建拓扑图。
///
/// * `nodes`    — 扫描到的节点列表
/// * `edges`    — 连接列表（可能有重复）
/// * `local_ip` — 本机 IP（用于标记本机节点）
pub fn build_graph(mut nodes: Vec<Node>, edges: Vec<Edge>, local_ip: &str) -> Graph {
    let start = Instant::now();
    let captured_at = Utc::now().to_rfc3339();

    // 去重合并 edges：相同 (src, dst, protocol, dst_port) 的连接累加 count
    let deduped_edges = dedup_edges(edges);

    // 标记本机节点
    for node in &mut nodes {
        if node.ip == local_ip {
            node.is_local = true;
        }
    }

    // 确保连接中涉及的 IP 都有对应节点（如果没有则补充占位节点）
    let known_ips: std::collections::HashSet<String> = nodes.iter().map(|n| n.ip.clone()).collect();
    let mut extra_nodes = Vec::new();
    for edge in &deduped_edges {
        for ip in [&edge.src, &edge.dst] {
            if !known_ips.contains(ip.as_str()) && !extra_nodes.iter().any(|n: &Node| &n.ip == ip) {
                extra_nodes.push(Node {
                    ip: ip.clone(),
                    hostname: None,
                    ports: Vec::new(),
                    is_local: ip == local_ip,
                    mac: None,
                    interface: None,
                });
            }
        }
    }
    nodes.extend(extra_nodes);

    // 构建 petgraph（主要用于后续查询；此处仅验证结构正确性）
    let mut _graph: DiGraph<String, String> = DiGraph::new();
    let mut node_indices: HashMap<String, petgraph::graph::NodeIndex> = HashMap::new();
    for node in &nodes {
        let idx = _graph.add_node(node.ip.clone());
        node_indices.insert(node.ip.clone(), idx);
    }
    for edge in &deduped_edges {
        if let (Some(&src_idx), Some(&dst_idx)) =
            (node_indices.get(&edge.src), node_indices.get(&edge.dst))
        {
            _graph.add_edge(
                src_idx,
                dst_idx,
                format!("{} x{}", edge.protocol, edge.count),
            );
        }
    }

    let tcp_connections = deduped_edges.iter().filter(|e| e.protocol == "TCP").count();
    let udp_connections = deduped_edges.iter().filter(|e| e.protocol == "UDP").count();
    let total_nodes = nodes.len();
    let total_edges = deduped_edges.len();
    let scan_duration_ms = start.elapsed().as_millis() as u64;

    Graph {
        nodes,
        edges: deduped_edges,
        captured_at,
        local_ip: local_ip.to_string(),
        summary: GraphSummary {
            total_nodes,
            total_edges,
            tcp_connections,
            udp_connections,
            scan_duration_ms,
        },
    }
}

/// 去重合并：相同 (src, dst, protocol, dst_port) 的连接累加 count
fn dedup_edges(edges: Vec<Edge>) -> Vec<Edge> {
    let mut map: HashMap<(String, String, String, u16), Edge> = HashMap::new();
    for edge in edges {
        let key = (
            edge.src.clone(),
            edge.dst.clone(),
            edge.protocol.clone(),
            edge.dst_port,
        );
        map.entry(key)
            .and_modify(|e| e.count += edge.count)
            .or_insert(edge);
    }
    let mut result: Vec<Edge> = map.into_values().collect();
    // 按 src/dst 排序以保证输出稳定
    result.sort_by(|a, b| {
        a.src
            .cmp(&b.src)
            .then(a.dst.cmp(&b.dst))
            .then(a.protocol.cmp(&b.protocol))
            .then(a.dst_port.cmp(&b.dst_port))
    });
    result
}

/// 按 `--min-connections` 过滤节点
pub fn filter_by_min_connections(graph: &mut Graph, min_connections: u32) {
    let keep: std::collections::HashSet<String> = graph
        .edges
        .iter()
        .filter(|e| e.count >= min_connections)
        .flat_map(|e| [e.src.clone(), e.dst.clone()])
        .collect();
    graph.nodes.retain(|n| n.is_local || keep.contains(&n.ip));
    graph
        .edges
        .retain(|e| keep.contains(&e.src) && keep.contains(&e.dst));
    graph.summary.total_nodes = graph.nodes.len();
    graph.summary.total_edges = graph.edges.len();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(ip: &str, is_local: bool) -> Node {
        Node {
            ip: ip.to_string(),
            hostname: None,
            ports: vec![],
            is_local,
            mac: None,
            interface: None,
        }
    }

    fn make_edge(src: &str, dst: &str, proto: &str, dst_port: u16, count: u32) -> Edge {
        Edge {
            src: src.to_string(),
            dst: dst.to_string(),
            protocol: proto.to_string(),
            src_port: 50000,
            dst_port,
            state: None,
            count,
        }
    }

    // ── 去重合并测试 ─────────────────────────────────────────────────────────

    #[test]
    fn test_dedup_merges_same_connection() {
        let edges = vec![
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 80, 1),
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 80, 1),
        ];
        let result = dedup_edges(edges);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].count, 2);
    }

    #[test]
    fn test_dedup_keeps_different_ports() {
        let edges = vec![
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 80, 1),
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 443, 1),
        ];
        let result = dedup_edges(edges);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_dedup_keeps_different_protocols() {
        let edges = vec![
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 80, 1),
            make_edge("10.0.0.1", "10.0.0.2", "UDP", 80, 1),
        ];
        let result = dedup_edges(edges);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_dedup_accumulates_count() {
        let edges = vec![
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 443, 1),
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 443, 1),
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 443, 1),
        ];
        let result = dedup_edges(edges);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].count, 3);
    }

    #[test]
    fn test_dedup_empty_input() {
        let result = dedup_edges(vec![]);
        assert!(result.is_empty());
    }

    // ── build_graph 测试 ─────────────────────────────────────────────────────

    #[test]
    fn test_build_graph_marks_local() {
        let nodes = vec![make_node("10.0.0.1", false)];
        let graph = build_graph(nodes, vec![], "10.0.0.1");
        assert!(graph.nodes[0].is_local);
    }

    #[test]
    fn test_build_graph_local_ip_field() {
        let graph = build_graph(vec![], vec![], "192.168.1.100");
        assert_eq!(graph.local_ip, "192.168.1.100");
    }

    #[test]
    fn test_build_graph_creates_missing_nodes_from_edges() {
        // Edge references IPs not in node list; build_graph should add placeholder nodes
        let edges = vec![make_edge("10.0.0.1", "10.0.0.2", "TCP", 80, 1)];
        let graph = build_graph(vec![], edges, "10.0.0.1");
        let ips: Vec<&str> = graph.nodes.iter().map(|n| n.ip.as_str()).collect();
        assert!(ips.contains(&"10.0.0.1"));
        assert!(ips.contains(&"10.0.0.2"));
    }

    #[test]
    fn test_build_graph_summary_counts() {
        let nodes = vec![
            make_node("192.168.1.100", true),
            make_node("192.168.1.1", false),
        ];
        let edges = vec![
            make_edge("192.168.1.100", "192.168.1.1", "TCP", 443, 1),
            make_edge("192.168.1.100", "8.8.8.8", "UDP", 53, 1),
        ];
        let graph = build_graph(nodes, edges, "192.168.1.100");
        // 2 explicit nodes + 1 placeholder for 8.8.8.8
        assert_eq!(graph.summary.total_nodes, 3);
        assert_eq!(graph.summary.total_edges, 2);
        assert_eq!(graph.summary.tcp_connections, 1);
        assert_eq!(graph.summary.udp_connections, 1);
    }

    #[test]
    fn test_build_graph_edge_dedup_in_pipeline() {
        // Two identical edges => merged to 1 with count=2
        let edges = vec![
            make_edge("192.168.1.100", "192.168.1.1", "TCP", 443, 1),
            make_edge("192.168.1.100", "192.168.1.1", "TCP", 443, 1),
        ];
        let graph = build_graph(vec![], edges, "192.168.1.100");
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].count, 2);
    }

    // ── filter_by_min_connections 测试 ───────────────────────────────────────

    #[test]
    fn test_filter_min_connections_removes_low_count_nodes() {
        let edges = vec![
            // A->B: count 1
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 80, 1),
            // A->C: count 3
            make_edge("10.0.0.1", "10.0.0.3", "TCP", 443, 3),
        ];
        let mut graph = build_graph(
            vec![
                make_node("10.0.0.1", false),
                make_node("10.0.0.2", false),
                make_node("10.0.0.3", false),
            ],
            edges,
            "10.0.0.99",
        );
        filter_by_min_connections(&mut graph, 2);
        let ips: Vec<&str> = graph.nodes.iter().map(|n| n.ip.as_str()).collect();
        // 10.0.0.2 is only connected via count=1 edge, should be removed
        assert!(!ips.contains(&"10.0.0.2"), "10.0.0.2 should be filtered");
        // 10.0.0.1 and 10.0.0.3 are in the count=3 edge
        assert!(ips.contains(&"10.0.0.1"));
        assert!(ips.contains(&"10.0.0.3"));
    }

    #[test]
    fn test_filter_min_connections_keeps_local_node() {
        // Local node (is_local=true) is always kept even if it has no high-count edges
        let edges = vec![make_edge("192.168.1.100", "192.168.1.1", "TCP", 80, 1)];
        let mut graph = build_graph(
            vec![
                make_node("192.168.1.100", true),
                make_node("192.168.1.1", false),
            ],
            edges,
            "192.168.1.100",
        );
        filter_by_min_connections(&mut graph, 5);
        let ips: Vec<&str> = graph.nodes.iter().map(|n| n.ip.as_str()).collect();
        // Local node should always remain
        assert!(ips.contains(&"192.168.1.100"));
    }

    #[test]
    fn test_filter_min_connections_updates_summary() {
        let edges = vec![
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 80, 5),
            make_edge("10.0.0.1", "10.0.0.3", "UDP", 53, 1),
        ];
        let mut graph = build_graph(vec![], edges, "10.0.0.99");
        let initial_nodes = graph.nodes.len();
        let initial_edges = graph.edges.len();
        filter_by_min_connections(&mut graph, 2);
        // After filtering, summary should be updated
        assert!(graph.summary.total_nodes <= initial_nodes);
        assert!(graph.summary.total_edges <= initial_edges);
        assert_eq!(graph.summary.total_nodes, graph.nodes.len());
        assert_eq!(graph.summary.total_edges, graph.edges.len());
    }

    // ── inbound/outbound 查询（验证 graph.edges 结构） ───────────────────────

    #[test]
    fn test_inbound_edges_query() {
        let edges = vec![
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 80, 1),
            make_edge("10.0.0.3", "10.0.0.2", "TCP", 443, 1),
            make_edge("10.0.0.1", "10.0.0.3", "TCP", 22, 1),
        ];
        let graph = build_graph(vec![], edges, "10.0.0.99");
        let inbound_2: Vec<_> = graph.edges.iter().filter(|e| e.dst == "10.0.0.2").collect();
        assert_eq!(inbound_2.len(), 2);
    }

    #[test]
    fn test_outbound_edges_query() {
        let edges = vec![
            make_edge("10.0.0.1", "10.0.0.2", "TCP", 80, 1),
            make_edge("10.0.0.1", "10.0.0.3", "TCP", 443, 1),
            make_edge("10.0.0.2", "10.0.0.3", "TCP", 22, 1),
        ];
        let graph = build_graph(vec![], edges, "10.0.0.99");
        let outbound_1: Vec<_> = graph.edges.iter().filter(|e| e.src == "10.0.0.1").collect();
        assert_eq!(outbound_1.len(), 2);
    }
}
