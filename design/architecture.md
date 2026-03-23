# netopo — 整体架构设计

> 本文档依据 CLAUDE.md 第三节（模块结构）和第四节（数据 Schema）编写，不得与之冲突。

---

## 一、项目定位

`netopo` 是一个纯 Rust CLI 工具，运行在终端内，无需云服务。它完成三件事：

1. **探测**：枚举本机网卡（F1）、扫描局域网设备和端口（F2）、抓取当前 TCP/UDP 连接快照（F3）
2. **构建**：将探测结果组装为有向图（F4）
3. **输出**：以 JSON / Graphviz dot / ASCII / TUI 四种格式呈现（F5–F8）

---

## 二、模块一览

CLAUDE.md 第三节锁定了以下七个源文件，职责边界严格不重叠：

```
src/
├── main.rs               CLI 入口：解析参数 + 按序调用各模块，禁止写业务逻辑
├── cli.rs                clap 参数定义，所有 --flag 在此声明
├── data_manager.rs       唯一数据类型定义（Node / Edge / Graph / GraphSummary）+ serde 实现
├── scanner.rs            F1 网卡枚举 + F2 子网扫描 / 端口探测
├── connection_tracker.rs F3 TCP/UDP 连接快照，三平台实现
├── graph_builder.rs      F4 petgraph 封装，Graph 构建与查询
└── visualization.rs      F5 JSON + F6 dot + F7 ASCII + F8 TUI 全部输出格式
```

---

## 三、模块依赖图

以下箭头表示"调用/依赖"方向（单向，无循环）：

```
main.rs
  │
  ├─── cli.rs                      (仅读取参数，无反向依赖)
  │
  ├─── scanner.rs ──────────────► data_manager.rs
  │         │                           ▲
  │         │ produces Node             │ all types defined here
  │         │                           │
  ├─── connection_tracker.rs ───────────┤
  │         │                           │
  │         │ produces Edge             │
  │         ▼                           │
  ├─── graph_builder.rs ────────────────┤
  │         │                           │
  │         │ produces Graph            │
  │         ▼                           │
  └─── visualization.rs ────────────────┘
             │
             ├── --output-json  →  serde_json
             ├── --output-dot   →  std::fs
             ├── --ascii        →  stdout
             └── --tui          →  ratatui + crossterm
```

**关键约束**：

- `data_manager.rs` 是所有类型的唯一来源；其他模块只能 `use crate::data_manager::*`，禁止在本地重新定义等价类型。
- `visualization.rs` 消费 `Graph`，不直接调用 `scanner` 或 `connection_tracker`。
- `main.rs` 是唯一协调者，负责将各模块的输出串联，但自身不包含业务逻辑。

---

## 四、数据流（端到端）

```
用户输入 CLI 参数
        │
        ▼
   cli.rs 解析 (clap)
        │
        ▼ CliArgs
   main.rs 协调
        │
        ├── 调用 scanner::enumerate_interfaces()
        │       └── 返回 Vec<Node>  (F1 网卡节点)
        │
        ├── 调用 scanner::scan_subnet(cidr, ports, concurrency, timeout)
        │       └── 返回 Vec<Node>  (F2 局域网设备)
        │
        ├── 调用 connection_tracker::snapshot()
        │       └── 返回 Vec<Edge>  (F3 连接快照)
        │
        ├── 调用 graph_builder::build(nodes, edges)
        │       └── 返回 Graph      (F4 有向图)
        │
        └── 调用 visualization::render(graph, output_opts)
                ├── write_json()    (F5)
                ├── write_dot()     (F6)
                ├── print_ascii()   (F7)
                └── run_tui()       (F8)
```

### 数据流中的类型转换说明

| 阶段 | 输入 | 输出 | 关键处理 |
|------|------|------|----------|
| scanner F1 | pnet `NetworkInterface` | `Vec<Node>` | 提取 ip/mac/interface，is_local=true |
| scanner F2 | CIDR 字符串 | `Vec<Node>` | 并发 TCP connect，DNS 反向解析 |
| connection_tracker | 系统调用 / 文件 | `Vec<Edge>` | 平台差异抹平，state 字段标准化 |
| graph_builder | `Vec<Node>` + `Vec<Edge>` | `Graph` | petgraph 去重，Edge.count 合并 |
| visualization | `Graph` | 输出产物 | 纯渲染，不修改数据 |

---

## 五、并发模型

- 运行时：`tokio`（`features = ["full"]`），全异步。
- `scanner::scan_subnet` 使用 `tokio::spawn` 并发探测，最多 `--concurrency`（默认 256）个并发连接。
- `connection_tracker::snapshot` 在 macOS 上通过 `tokio::process::Command` 异步执行 `netstat`，避免阻塞主线程。
- TUI 事件循环运行在主线程，后台刷新使用 `tokio::time::interval`。

---

## 六、错误处理策略

- 全局使用 `anyhow::Result<T>` 作为函数返回类型（非测试代码中禁止裸 `unwrap()`）。
- 自定义错误类型通过 `thiserror` 派生，定义在对应模块内。
- 权限不足时（如 `netstat` 需要 root 的场景）捕获错误后打印友好提示，不 panic。

---

## 七、平台差异矩阵

| 功能 | macOS | Linux | Windows |
|------|-------|-------|---------|
| 网卡枚举 | `pnet::datalink::interfaces()` | 同左 | 同左 |
| 子网扫描 | TCP connect（无 ICMP，无 root） | 同左 | 同左 |
| 连接追踪 | `netstat -an -p tcp/udp` 解析 | `/proc/net/tcp` 解析 | WinAPI `GetExtendedTcpTable` |
| TUI | crossterm + ratatui | 同左 | 同左 |

当前开发平台为 **macOS**，`connection_tracker.rs` 的 macOS 路径为主要实现路径，详见 `design/api_contracts.md` 第三节。

---

## 八、目录结构（完整）

```
netopo/
├── Cargo.toml
├── CLAUDE.md              项目总控文档（最高权威）
├── README.md
├── src/
│   ├── main.rs
│   ├── cli.rs
│   ├── data_manager.rs
│   ├── scanner.rs
│   ├── connection_tracker.rs
│   ├── graph_builder.rs
│   └── visualization.rs
├── tests/                 集成测试
├── design/                架构设计文档（本目录）
│   ├── architecture.md    本文件
│   ├── api_contracts.md
│   └── test_plan.md
├── docs/
│   ├── progress.md
│   ├── structure.md
│   ├── tutorial.md
│   └── lesson_learned.md
└── reports/
