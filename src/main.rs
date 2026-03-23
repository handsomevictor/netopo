mod cli;
mod connection_tracker;
mod data_manager;
mod graph_builder;
mod scanner;
mod visualization;

use clap::Parser;
use cli::Cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 解析端口列表
    let ports = cli.parse_ports()?;

    // 确定本机 IP
    let local_ip = scanner::local_ip().unwrap_or_else(|| "127.0.0.1".to_string());

    // ── 扫描模式 ──────────────────────────────────────────────────────────────
    let mut nodes = Vec::new();

    if cli.scan {
        // F1: 枚举本机网卡
        match scanner::scan_interfaces().await {
            Ok(iface_nodes) => {
                if !cli.quiet {
                    eprintln!("发现 {} 个本机接口", iface_nodes.len());
                }
                nodes.extend(iface_nodes);
            }
            Err(e) => {
                eprintln!("网卡枚举失败: {}", e);
            }
        }

        // F2: 扫描子网
        let cidr = cli
            .subnet
            .clone()
            .or_else(scanner::default_cidr)
            .unwrap_or_else(|| "192.168.1.0/24".to_string());

        if !cli.quiet {
            eprintln!("扫描子网: {} ...", cidr);
        }
        match scanner::scan_subnet(&cidr, &ports, cli.concurrency, cli.timeout).await {
            Ok(subnet_nodes) => {
                if !cli.quiet {
                    eprintln!("发现 {} 个活跃主机", subnet_nodes.len());
                }
                nodes.extend(subnet_nodes);
            }
            Err(e) => {
                eprintln!("子网扫描失败: {}", e);
            }
        }
    }

    // ── 连接追踪 ──────────────────────────────────────────────────────────────
    let mut edges = Vec::new();

    if cli.connections || cli.graph || cli.ascii || cli.tui {
        match connection_tracker::get_connections(cli.local_only, cli.exclude_loopback).await {
            Ok(conns) => {
                if !cli.quiet {
                    eprintln!("获取到 {} 条活跃连接", conns.len());
                }
                edges.extend(conns);
            }
            Err(e) => {
                eprintln!("连接追踪失败（可能需要更高权限）: {}", e);
            }
        }
    }

    // ── 构建拓扑图 ────────────────────────────────────────────────────────────
    if nodes.is_empty() && !edges.is_empty() {
        // 只有连接数据时，补充占位节点
    }

    let mut graph = graph_builder::build_graph(nodes, edges, &local_ip);

    // 按最小连接数过滤
    if let Some(min_conn) = cli.min_connections {
        graph_builder::filter_by_min_connections(&mut graph, min_conn);
    }

    // ── 输出 ──────────────────────────────────────────────────────────────────

    if let Some(ref json_file) = cli.output_json {
        visualization::output_json(&graph, json_file)?;
    }

    if let Some(ref dot_file) = cli.output_dot {
        visualization::output_dot(&graph, dot_file)?;
    }

    if cli.ascii {
        visualization::print_ascii(&graph);
    }

    if cli.tui {
        visualization::run_tui(graph.clone(), local_ip.clone())?;
    }

    // ── watch 模式 ────────────────────────────────────────────────────────────
    if let Some(interval) = cli.watch {
        if !cli.quiet {
            eprintln!("watch 模式：每 {} 秒刷新（Ctrl+C 退出）", interval);
        }
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
            match connection_tracker::get_connections(cli.local_only, cli.exclude_loopback).await {
                Ok(conns) => {
                    let new_graph = graph_builder::build_graph(vec![], conns, &local_ip);
                    if cli.ascii {
                        visualization::print_ascii(&new_graph);
                    }
                }
                Err(e) => eprintln!("刷新失败: {}", e),
            }
        }
    }

    // 如果什么都没指定，打印帮助提示
    if !cli.scan
        && !cli.connections
        && !cli.graph
        && !cli.ascii
        && !cli.tui
        && cli.output_json.is_none()
        && cli.output_dot.is_none()
    {
        eprintln!("提示: 使用 --help 查看所有选项。例如: netopo --scan --ascii");
    }

    Ok(())
}
