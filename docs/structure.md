# 项目结构

> 本文档由 doc_agent 在每轮迭代后维护，反映当前实际状态。
> 最后更新：轮次 1（2026-03-23）

---

## 目录结构

```
netopo/
├── src/
│   ├── main.rs              # CLI 入口
│   ├── cli.rs               # clap 参数定义
│   ├── data_manager.rs      # 数据类型定义
│   ├── scanner.rs           # F1 + F2：网卡枚举与子网扫描
│   ├── connection_tracker.rs # F3：TCP/UDP 连接追踪
│   ├── graph_builder.rs     # F4：拓扑图构建
│   └── visualization.rs     # F5 + F6 + F7 + F8：输出格式
├── docs/
│   ├── tutorial.md          # 功能使用教程
│   ├── structure.md         # 本文档
│   ├── lesson_learned.md    # 经验教训
│   └── progress.md          # 迭代进度
├── Cargo.toml
├── CLAUDE.md                # 项目总控文档（最高权威）
└── README.md
```

---

## 模块详细说明

### `src/main.rs` — CLI 入口

**职责：** 解析命令行参数、按顺序组装各模块调用、控制执行流程。

**实现状态：已实现**

实现细节：
- 通过 `Cli::parse()` 解析参数，调用 `cli.parse_ports()` 解析端口列表
- `scanner::local_ip()` 获取本机主接口 IP
- 按 `--scan` → `--connections` → `build_graph` → 各输出格式的顺序执行
- `--watch` 模式用 `tokio::time::sleep` 循环实现
- 无命令参数时打印使用提示
- 禁止在此文件写业务逻辑，仅组装调用

---

### `src/cli.rs` — CLI 参数定义

**职责：** 声明所有 `--flag`，提供端口字符串解析。

**实现状态：已实现**

实现细节：
- 使用 `clap` derive 宏定义 `Cli` 结构体
- 参数分组：扫描选项、连接选项、图构建、输出格式、通用
- `parse_ports()` 支持逗号分隔格式（`"22,80,443"`）和范围格式（`"1-1024"`），两者可混用
- 默认端口列表：21, 22, 23, 25, 53, 80, 110, 143, 443, 445, 3306, 3389, 5432, 5900, 6379, 8080, 8443, 8888, 9200, 27017

---

### `src/data_manager.rs` — 数据类型定义

**职责：** 全项目唯一的数据类型来源，其他模块不得自定义等价类型。

**实现状态：已实现**

定义的类型：
- `Node`：ip, hostname, ports, is_local, mac, interface
- `Edge`：src, dst, protocol, src_port, dst_port, state, count
- `Graph`：nodes, edges, captured_at, local_ip, summary
- `GraphSummary`：total_nodes, total_edges, tcp_connections, udp_connections, scan_duration_ms

全部实现了 `serde::{Serialize, Deserialize}`，可直接用于 JSON 输出。

---

### `src/scanner.rs` — 网卡枚举与子网扫描（F1 + F2）

**职责：** 枚举本机网络接口（F1）、并发扫描子网设备和开放端口（F2）。

**实现状态：已实现**

实现细节：

**F1 — `scan_interfaces()`：**
- 调用 `pnet::datalink::interfaces()` 枚举所有接口
- 跳过 IPv6 地址（当前版本仅处理 IPv4）
- 调用 `detect_primary_interface()` 识别主接口并打标

**F1 — `detect_primary_interface()`：**
- macOS：执行 `route -n get default`，提取 `interface:` 行
- Linux：执行 `ip route show default`，提取 `dev <name>` token

**F2 — `scan_subnet()`：**
- 解析 CIDR 获取所有主机 IP
- 为每个 IP 派发 `tokio::spawn` 任务，用 `Semaphore` 控制并发数
- 对每个 IP 并发探测所有指定端口（TCP connect），记录开放端口
- 有开放端口时调用 `reverse_lookup()` 做 DNS 反向解析
- 主机无任何开放端口时不创建节点

**辅助函数：**
- `default_cidr()`：从主接口 IP 推算所在子网的 CIDR（直接使用 pnet 返回的 IpNet）
- `local_ip()`：返回主接口的 IPv4 地址字符串

---

### `src/connection_tracker.rs` — TCP/UDP 连接追踪（F3）

**职责：** 跨平台获取当前主机活跃的 TCP/UDP 连接快照。

**实现状态：已实现（macOS + Linux）**

实现细节：

**macOS 实现：**
- 执行 `netstat -an -p tcp` 和 `netstat -an -p udp` 获取输出
- `parse_netstat_line()` 解析文本，支持 IPv4（`a.b.c.d.port`）和 IPv6（`[addr].port`）格式
- 跳过通配符条目（`*.*`）

**Linux 实现：**
- 读取 `/proc/net/tcp`、`/proc/net/tcp6`、`/proc/net/udp`
- `parse_hex_addr()` 解析十六进制地址，处理 little-endian 字节序
- `decode_tcp_state()` 将十六进制状态码转换为可读字符串（如 `"01"` → `"ESTABLISHED"`）

**过滤逻辑：**
- `--local-only`：只保留 dst 为私有地址的连接
- `--exclude-loopback`：过滤 src 或 dst 为 127.x.x.x 或 ::1 的连接

**Windows：** 运行时返回 `bail!` 错误，提示当前平台暂不支持。

**测试：** 有 macOS 和 Linux 各自的单元测试，覆盖正常解析和通配符跳过场景。

---

### `src/graph_builder.rs` — 拓扑图构建（F4）

**职责：** 将 Node 和 Edge 列表构建为 petgraph 有向图，去重合并，标记本机节点。

**实现状态：已实现**

实现细节：

**`build_graph()`：**
- 对 `edges` 执行 `dedup_edges()`：相同 (src, dst, protocol, dst_port) 的连接累加 `count`
- 遍历 `nodes` 标记 `is_local`（与 `local_ip` 相同的节点）
- 对 `deduped_edges` 中涉及的 IP，如果没有对应 `Node`，补充占位节点（ports 为空）
- 构建 `petgraph::graph::DiGraph<String, String>`，边标签为 `"TCP x3"` 形式

**`dedup_edges()`：**
- 用 `HashMap<(src, dst, protocol, dst_port), Edge>` 去重并累加 `count`
- 结果按 (src, dst, protocol, dst_port) 排序，保证输出稳定

**`filter_by_min_connections()`：**
- 收集 `count >= min_connections` 的连接所涉及的 IP 集合
- 保留本机节点及集合内 IP 的节点，删除其余节点和不合条件的边
- 更新 `summary` 中的 `total_nodes` 和 `total_edges`

---

### `src/visualization.rs` — 输出格式（F5 + F6 + F7 + F8）

**职责：** 实现全部四种输出格式。

**实现状态：已实现**

**F5 — `output_json()`：**
- `serde_json::to_string_pretty()` 序列化 `Graph`
- 写入指定文件，失败时返回带上下文的错误

**F6 — `output_dot()`：**
- 手动拼接 dot 字符串，逐节点逐边生成
- 本机节点：`shape=doublecircle color=blue`
- 边 penwidth = `max(1.0, count * 0.5).min(5.0)`
- UDP 边添加 `style=dashed`
- 节点标签：IP + hostname + `N ports`，用 `\n` 分隔

**F7 — `print_ascii()`：**
- 打印带边框的标题（含扫描时间）
- 以本机节点为根，打印出站连接树
- 连接箭头：`──TCP:PORT──►` 或 `╌╌UDP:PORT╌╌►`
- 每行调用 `truncate(s, 80)` 截断，保证 80 列不溢出
- 末行汇总节点数和连接数

**F8 — `run_tui()` / `run_tui_loop()` / `draw_ui()`：**
- 四区域布局：标题栏(1行) + 中间区域(节点列表+连接详情) + 拓扑图(4行) + 状态栏(1行)
- 节点列表：左侧 40%，带选中高亮、颜色分级（本机蓝色、无端口灰色）
- 连接详情：右侧 60%，显示选中节点的入站/出站数（绿色）、开放端口、hostname
- 拓扑图：显示本机节点到出站目标的简化 ASCII 连接图
- 状态栏：默认显示快捷键，过滤/导出模式时显示输入提示
- 实现全部 7 个交互键：q、Ctrl+C、r、↑↓、f、e、Tab、?

---

## TODO / 待改进

| 模块 | 问题 | 优先级 |
|------|------|--------|
| scanner.rs | IPv6 地址跳过，不参与子网扫描 | 低 |
| connection_tracker.rs | Windows 平台未实现 | 中 |
| visualization.rs | TUI `r` 刷新键目前只显示提示，不真正重新扫描 | 高 |
| visualization.rs | TUI 拓扑图面板只显示本机出站连接，不显示完整网络 | 中 |
| scanner.rs | `default_cidr()` 直接用 pnet IpNet 的 CIDR，可能不是 /24 | 低 |
