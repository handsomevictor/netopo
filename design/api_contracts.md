# netopo — 模块间 API 契约

> 本文档严格遵守 CLAUDE.md 第三节（模块结构）和第四节（数据 Schema）。
> 所有类型定义以第四节为准，本文档只做引用，不重新定义。

---

## 一、data_manager.rs — 全项目唯一类型来源

以下类型定义直接引用自 CLAUDE.md 第四节，developer_agent 实现时必须与此完全一致，不得修改字段名、类型或可见性。

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub ip: String,
    pub hostname: Option<String>,
    pub ports: Vec<u16>,
    pub is_local: bool,
    pub mac: Option<String>,
    pub interface: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub src: String,             // 源 IP（不含端口）
    pub dst: String,             // 目标 IP（不含端口）
    pub protocol: String,        // "TCP" | "UDP"
    pub src_port: u16,
    pub dst_port: u16,
    pub state: Option<String>,   // "ESTABLISHED" | "TIME_WAIT" | "LISTEN" | ...
    pub count: u32,              // 合并后的连接数，初始为 1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub captured_at: String,     // RFC3339 格式，如 "2024-01-15T10:30:00+08:00"
    pub local_ip: String,        // 本机主接口 IP
    pub summary: GraphSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphSummary {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub tcp_connections: usize,
    pub udp_connections: usize,
    pub scan_duration_ms: u64,
}
```

---

## 二、cli.rs — 参数定义契约

`cli.rs` 只定义 `CliArgs` 结构体，由 `main.rs` 通过 `clap::Parser::parse()` 实例化后传递给各模块。

```rust
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "netopo", version = "0.1.0")]
#[command(about = "Local network topology mapper")]
pub struct CliArgs {
    // 扫描选项
    #[arg(long)]
    pub scan: bool,

    #[arg(long, value_name = "CIDR")]
    pub subnet: Option<String>,

    /// 端口列表，支持 "22,80,443" 或 "1-1024"
    #[arg(long, value_name = "LIST")]
    pub ports: Option<String>,

    #[arg(long, default_value_t = 256)]
    pub concurrency: usize,

    #[arg(long, default_value_t = 500)]
    pub timeout: u64,

    // 连接选项
    #[arg(long)]
    pub connections: bool,

    #[arg(long)]
    pub local_only: bool,

    #[arg(long)]
    pub exclude_loopback: bool,

    #[arg(long, value_name = "SECONDS")]
    pub watch: Option<u64>,

    // 图构建
    #[arg(long)]
    pub graph: bool,

    #[arg(long, value_name = "N")]
    pub min_connections: Option<u32>,

    // 输出格式
    #[arg(long, value_name = "FILE")]
    pub output_json: Option<std::path::PathBuf>,

    #[arg(long, value_name = "FILE")]
    pub output_dot: Option<std::path::PathBuf>,

    #[arg(long)]
    pub ascii: bool,

    #[arg(long)]
    pub tui: bool,

    // 通用
    #[arg(short, long)]
    pub verbose: bool,

    #[arg(short, long)]
    pub quiet: bool,
}
```

---

## 三、scanner.rs — 公开 API

### 3.1 F1：网卡枚举

```rust
/// 枚举本机所有网络接口，返回 Node 列表。
/// 每个 Node 的 is_local = true，interface 字段填写接口名。
/// 主接口（有默认路由的那个）的 Node 同时作为 Graph.local_ip 的来源。
///
/// 实现依赖：pnet::datalink::interfaces()
pub fn enumerate_interfaces() -> anyhow::Result<Vec<Node>>
```

返回值说明：
- `Node.ip`：接口 IPv4 地址（字符串形式，如 "192.168.1.100"）
- `Node.mac`：接口 MAC 地址（如 "aa:bb:cc:dd:ee:ff"），无 MAC 时为 None
- `Node.interface`：接口名（如 "en0"、"eth0"）
- `Node.is_local`：始终为 `true`
- `Node.ports`：始终为空 `Vec`（网卡枚举不探测端口）
- `Node.hostname`：尝试本机 hostname 解析，失败则 None

### 3.2 F2：子网扫描

```rust
/// 对给定 CIDR 进行并发 TCP connect 扫描。
///
/// # 参数
/// - cidr: 扫描网段，如 "192.168.1.0/24"
/// - ports: 要探测的端口列表
/// - concurrency: 最大并发连接数
/// - timeout_ms: 单连接超时（毫秒）
///
/// # 返回
/// 所有有响应 IP 的 Node 列表（不含本机接口，本机由 enumerate_interfaces 负责）
pub async fn scan_subnet(
    cidr: &str,
    ports: &[u16],
    concurrency: usize,
    timeout_ms: u64,
) -> anyhow::Result<Vec<Node>>
```

默认端口列表（CLAUDE.md F2 规定）：
```rust
pub const DEFAULT_PORTS: &[u16] = &[
    21, 22, 23, 25, 53, 80, 110, 143, 443, 445,
    3306, 3389, 5432, 5900, 6379, 8080, 8443, 8888, 9200, 27017,
];
```

### 3.3 端口字符串解析（内部工具函数）

```rust
/// 将 "22,80,443" 或 "1-1024" 格式的字符串解析为端口列表
pub fn parse_ports(input: &str) -> anyhow::Result<Vec<u16>>
```

---

## 四、connection_tracker.rs — 公开 API

### 4.1 主接口

```rust
/// 获取当前主机上所有活跃的 TCP/UDP 连接快照。
/// 跨平台实现，由条件编译选择具体路径。
pub async fn snapshot() -> anyhow::Result<Vec<Edge>>
```

### 4.2 过滤选项

```rust
pub struct SnapshotOptions {
    pub local_only: bool,
    pub exclude_loopback: bool,
}

pub async fn snapshot_with_opts(opts: SnapshotOptions) -> anyhow::Result<Vec<Edge>>
```

### 4.3 macOS 实现细节（当前开发平台）

**命令**：

```bash
netstat -an -p tcp
netstat -an -p udp
```

**典型 TCP 输出行**（macOS `netstat -an -p tcp`）：

```
Proto Recv-Q Send-Q  Local Address          Foreign Address        (state)
tcp4       0      0  192.168.1.100.443      192.168.1.1.52341      ESTABLISHED
tcp4       0      0  *.8080                 *.*                    LISTEN
tcp6       0      0  *.22                   *.*                    LISTEN
```

**解析规则**：

1. 跳过 `Proto` 开头的表头行和空行。
2. 字段分割：按空白符 split，取索引 [0]=proto, [3]=local, [4]=foreign, [5]=state（TCP）；UDP 无 state 字段。
3. 地址解析：macOS 格式为 `IP.PORT`（最后一个 `.` 前为 IP，最后一个 `.` 后为 PORT）。
   - IPv6 地址形如 `::1.PORT`，需单独处理。
   - 通配符 `*.*` 或 `*.PORT` 表示监听所有接口，IP 替换为 `"0.0.0.0"`。
4. 端口解析：`netstat` 可能输出服务名（如 `https`）而非数字，需通过 `getservbyname` 或静态映射转换。建议启动时加 `-n` 参数强制数字格式（`netstat -an` 已包含 `-n`）。
5. 状态字段：TCP 取第 6 列，UDP 填 `None`。
6. `Edge.count` 初始为 `1`，由 `graph_builder` 负责合并。

**macOS 特有注意事项**：

- macOS 的 `netstat` 默认输出 `tcp4`/`tcp6` 而非 `tcp`，proto 字段需忽略数字后缀，统一归类为 `"TCP"` / `"UDP"`。
- `netstat -an -p tcp` 与 `-p udp` 需分两次执行，用 `tokio::process::Command` 异步调用。
- 若进程无权限读取某些 socket 信息，`netstat` 会跳过该行而非报错，实现时无需特殊处理权限错误。
- TIME_WAIT 状态的连接在 macOS 上可能大量出现，调用方可通过 `SnapshotOptions` 决定是否过滤。

**Rust 实现骨架（macOS 路径）**：

```rust
#[cfg(target_os = "macos")]
async fn netstat_parse(proto_flag: &str) -> anyhow::Result<Vec<Edge>> {
    use tokio::process::Command;

    let output = Command::new("netstat")
        .args(["-an", "-p", proto_flag])
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let protocol = proto_flag.to_uppercase(); // "tcp" -> "TCP"

    let mut edges = Vec::new();
    for line in stdout.lines().skip(2) { // 跳过两行表头
        if let Some(edge) = parse_netstat_line(line, &protocol) {
            edges.push(edge);
        }
    }
    Ok(edges)
}

fn parse_netstat_line(line: &str, protocol: &str) -> Option<Edge> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    // TCP: fields[0]=proto, [3]=local, [4]=foreign, [5]=state
    // UDP: fields[0]=proto, [3]=local, [4]=foreign  (无 state)
    // ...
}

fn parse_macos_addr(addr: &str) -> Option<(String, u16)> {
    // 按最后一个 '.' 分割 IP 和 PORT
    let last_dot = addr.rfind('.')?;
    let ip = &addr[..last_dot];
    let port: u16 = addr[last_dot + 1..].parse().ok()?;
    let ip = if ip == "*" { "0.0.0.0".to_string() } else { ip.to_string() };
    Some((ip, port))
}
```

### 4.4 Linux 实现细节（参考）

读取 `/proc/net/tcp`、`/proc/net/tcp6`、`/proc/net/udp`，使用 `procfs` crate 或手动解析十六进制地址。地址格式为小端序十六进制，需转换为点分十进制。

### 4.5 Windows 实现细节（参考）

通过 `winapi` crate 调用 `GetExtendedTcpTable` / `GetExtendedUdpTable`，直接返回结构化数据，无需文本解析。

---

## 五、graph_builder.rs — 公开 API

```rust
use petgraph::Graph as PetGraph;
use petgraph::Directed;

/// 由 nodes + edges 构建内部 petgraph，同时返回经去重合并的 Graph 结构。
///
/// 去重规则：
/// - Node 以 ip 为唯一键；相同 ip 的多个 Node 合并（ports 取并集）。
/// - Edge 以 (src, dst, protocol, dst_port) 为唯一键；相同 key 的边 count 累加。
pub fn build(nodes: Vec<Node>, edges: Vec<Edge>) -> anyhow::Result<Graph>

/// 查询指定 IP 的所有入站连接
pub fn inbound_edges<'a>(graph: &'a Graph, ip: &str) -> Vec<&'a Edge>

/// 查询指定 IP 的所有出站连接
pub fn outbound_edges<'a>(graph: &'a Graph, ip: &str) -> Vec<&'a Edge>

/// 按最小连接数过滤节点（--min-connections）
pub fn filter_by_min_connections(graph: &mut Graph, min: u32)

/// 返回内部 petgraph 引用（供需要图算法的场景使用）
pub fn petgraph(graph: &Graph) -> PetGraph<String, u32, Directed>
```

---

## 六、visualization.rs — 公开 API

```rust
/// F5：将 Graph 序列化为 JSON 写入文件
pub fn write_json(graph: &Graph, path: &std::path::Path) -> anyhow::Result<()>

/// F6：将 Graph 渲染为 Graphviz dot 格式写入文件
pub fn write_dot(graph: &Graph, path: &std::path::Path) -> anyhow::Result<()>

/// F7：在当前终端打印 ASCII 拓扑图
pub fn print_ascii(graph: &Graph) -> anyhow::Result<()>

/// F8：启动 ratatui TUI 交互界面（阻塞直到用户退出）
pub fn run_tui(graph: Graph) -> anyhow::Result<()>
```

### Dot 渲染规范（F6）

- 本机节点：`shape=doublecircle color=blue`
- 边的 `penwidth`：`1.0 + (edge.count as f32 * 0.5)`，上限 5.0
- TCP 连接：实线（默认）；UDP 连接：`style=dashed`
- 节点标签：`IP\nhostname（如有）\nN ports`
- 全局：`rankdir=LR; node [fontname="Helvetica" fontsize=10]`

### ASCII 渲染规范（F7）

- 本机节点标识：`[★ IP]`；其他节点：`[IP]`
- TCP 连接箭头：`──TCP:PORT──►`
- UDP 连接箭头：`╌╌UDP:PORT╌╌►`
- 整体宽度不超过 80 列
- 标题框使用 `╔══╗ ║ ╚══╝` 边框

### TUI 布局规范（F8）

```
┌─────────────────────────────────────────────────────────┐  <- 标题栏 bg=Blue fg=White
├────────────────────────┬────────────────────────────────┤
│  节点列表面板           │  连接详情面板                   │  <- 上半区，左右分割
├────────────────────────┴────────────────────────────────┤
│  拓扑图面板（ASCII 渲染）                                 │  <- 中间区
├─────────────────────────────────────────────────────────┤
│  状态栏 bg=Black fg=Green                                │  <- 底部
└─────────────────────────────────────────────────────────┘
```

颜色方案（严格实现）：
- 本机节点：`Color::LightBlue` + `Modifier::BOLD`
- 活跃 TCP 连接数：`Color::Green`
- UDP 连接数：`Color::Yellow`
- 无响应节点：`Color::DarkGray`
- 选中行：`Modifier::REVERSED`

---

## 七、模块间调用约束汇总

| 调用方 | 被调用方 | 允许 | 禁止 |
|--------|---------|------|------|
| main.rs | 所有模块 | 任意调用 | 包含业务逻辑 |
| scanner.rs | data_manager.rs | 使用 Node | 定义自己的节点类型 |
| connection_tracker.rs | data_manager.rs | 使用 Edge | 定义自己的边类型 |
| graph_builder.rs | data_manager.rs | 使用 Graph/Node/Edge | — |
| visualization.rs | data_manager.rs | 只读 Graph | 修改 Graph 数据 |
| visualization.rs | graph_builder.rs | 调用查询函数 | 直接操作 petgraph |
| 任意模块 | cli.rs | 禁止 | 除 main.rs 外不得直接读取 CliArgs |
