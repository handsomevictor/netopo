//! data_manager — 全项目唯一数据类型定义（Node/Edge/Graph）+ serde 实现

use serde::{Deserialize, Serialize};

/// 网络节点（一台设备或一个接口）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub ip: String,
    pub hostname: Option<String>,
    pub ports: Vec<u16>,
    pub is_local: bool,
    pub mac: Option<String>,
    pub interface: Option<String>,
}

/// 两个节点之间的一条有向连接
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub src: String,
    pub dst: String,
    /// "TCP" | "UDP"
    pub protocol: String,
    pub src_port: u16,
    pub dst_port: u16,
    /// "ESTABLISHED" | "TIME_WAIT" | ...
    pub state: Option<String>,
    /// 合并后的连接数
    pub count: u32,
}

/// 整张拓扑图
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// RFC3339 时间戳
    pub captured_at: String,
    pub local_ip: String,
    pub summary: GraphSummary,
}

/// 统计摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSummary {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub tcp_connections: usize,
    pub udp_connections: usize,
    pub scan_duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node() -> Node {
        Node {
            ip: "192.168.1.1".to_string(),
            hostname: Some("router.local".to_string()),
            ports: vec![80, 443],
            is_local: false,
            mac: Some("aa:bb:cc:dd:ee:ff".to_string()),
            interface: None,
        }
    }

    fn make_edge() -> Edge {
        Edge {
            src: "192.168.1.100".to_string(),
            dst: "192.168.1.1".to_string(),
            protocol: "TCP".to_string(),
            src_port: 54321,
            dst_port: 443,
            state: Some("ESTABLISHED".to_string()),
            count: 1,
        }
    }

    fn make_graph() -> Graph {
        Graph {
            nodes: vec![make_node()],
            edges: vec![make_edge()],
            captured_at: "2024-01-15T10:30:00+08:00".to_string(),
            local_ip: "192.168.1.100".to_string(),
            summary: GraphSummary {
                total_nodes: 1,
                total_edges: 1,
                tcp_connections: 1,
                udp_connections: 0,
                scan_duration_ms: 500,
            },
        }
    }

    #[test]
    fn test_node_serde_roundtrip() {
        let node = make_node();
        let json = serde_json::to_string(&node).unwrap();
        let decoded: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(node.ip, decoded.ip);
        assert_eq!(node.hostname, decoded.hostname);
        assert_eq!(node.ports, decoded.ports);
        assert_eq!(node.is_local, decoded.is_local);
        assert_eq!(node.mac, decoded.mac);
        assert_eq!(node.interface, decoded.interface);
    }

    #[test]
    fn test_node_optional_fields_none() {
        let node = Node {
            ip: "10.0.0.1".to_string(),
            hostname: None,
            ports: vec![],
            is_local: true,
            mac: None,
            interface: None,
        };
        let json = serde_json::to_string(&node).unwrap();
        let decoded: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.hostname, None);
        assert_eq!(decoded.mac, None);
        assert!(decoded.ports.is_empty());
    }

    #[test]
    fn test_edge_protocol_tcp_serialized() {
        let edge = make_edge();
        let json = serde_json::to_string(&edge).unwrap();
        assert!(json.contains("\"TCP\""));
        assert!(json.contains("\"ESTABLISHED\""));
    }

    #[test]
    fn test_edge_serde_roundtrip() {
        let edge = make_edge();
        let json = serde_json::to_string(&edge).unwrap();
        let decoded: Edge = serde_json::from_str(&json).unwrap();
        assert_eq!(edge.src, decoded.src);
        assert_eq!(edge.dst, decoded.dst);
        assert_eq!(edge.protocol, decoded.protocol);
        assert_eq!(edge.src_port, decoded.src_port);
        assert_eq!(edge.dst_port, decoded.dst_port);
        assert_eq!(edge.state, decoded.state);
        assert_eq!(edge.count, decoded.count);
    }

    #[test]
    fn test_edge_udp_no_state() {
        let edge = Edge {
            src: "192.168.1.100".to_string(),
            dst: "8.8.8.8".to_string(),
            protocol: "UDP".to_string(),
            src_port: 63412,
            dst_port: 53,
            state: None,
            count: 1,
        };
        let json = serde_json::to_string(&edge).unwrap();
        let decoded: Edge = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.state, None);
        assert_eq!(decoded.protocol, "UDP");
    }

    #[test]
    fn test_graph_serde_roundtrip() {
        let graph = make_graph();
        let json = serde_json::to_string(&graph).unwrap();
        let decoded: Graph = serde_json::from_str(&json).unwrap();
        assert_eq!(graph.local_ip, decoded.local_ip);
        assert_eq!(graph.captured_at, decoded.captured_at);
        assert_eq!(graph.nodes.len(), decoded.nodes.len());
        assert_eq!(graph.edges.len(), decoded.edges.len());
    }

    #[test]
    fn test_graph_captured_at_rfc3339() {
        let graph = make_graph();
        assert!(
            chrono::DateTime::parse_from_rfc3339(&graph.captured_at).is_ok(),
            "captured_at must be valid RFC3339: {}",
            graph.captured_at
        );
    }

    #[test]
    fn test_graph_summary_serde_roundtrip() {
        let summary = GraphSummary {
            total_nodes: 5,
            total_edges: 8,
            tcp_connections: 6,
            udp_connections: 2,
            scan_duration_ms: 1234,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let decoded: GraphSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(summary.total_nodes, decoded.total_nodes);
        assert_eq!(summary.total_edges, decoded.total_edges);
        assert_eq!(summary.tcp_connections, decoded.tcp_connections);
        assert_eq!(summary.udp_connections, decoded.udp_connections);
        assert_eq!(summary.scan_duration_ms, decoded.scan_duration_ms);
    }

    #[test]
    fn test_graph_json_has_required_fields() {
        let graph = make_graph();
        let json = serde_json::to_string(&graph).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(val.get("nodes").is_some());
        assert!(val.get("edges").is_some());
        assert!(val.get("captured_at").is_some());
        assert!(val.get("local_ip").is_some());
        assert!(val.get("summary").is_some());
    }
}
