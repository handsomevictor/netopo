# 代码审查报告 — 轮次 1

审查日期：2026-03-23
审查人：reviewer_agent
项目路径：`/Users/handsomevictor/Documents/GitHub/netopo`

---

## 架构合规性

对照 CLAUDE.md 第三节"模块结构（锁定）"逐项检查。

### main.rs — CLI 入口
**状态：基本合规，存在轻微违规**

`main.rs` 主要负责参数解析和模块调用，符合"禁止写业务逻辑"的要求。但以下内容越界：

- 第 79-81 行存在占位注释块，虽然没有实际逻辑，但该补充占位节点的职责应属于 `graph_builder.rs`。
- watch 循环（第 109-125 行）直接在 `main.rs` 中调用 `connection_tracker`、`graph_builder`、`visualization` 并判断 `cli.ascii`，包含了轻量级的流程编排业务逻辑，属于边界案例。严格来说，watch 模式的调度逻辑应封装为独立函数或模块。
- `scanner::local_ip()` 的 `unwrap_or_else` 回落逻辑（第 19 行）是可接受的。

### 各模块类型定义
**状态：合规**

- `cli.rs`：仅定义 `Cli` struct 和 `parse_ports` 方法，无数据类型定义。
- `scanner.rs`：使用 `crate::data_manager::Node`，未自定义等价类型。
- `connection_tracker.rs`：使用 `crate::data_manager::Edge`，未自定义等价类型。
- `graph_builder.rs`：使用 `data_manager` 的全套类型，未自定义等价类型。
- `visualization.rs`：导入 `crate::data_manager::{Edge, Graph, Node}`，未自定义等价类型。

### data_manager.rs — 唯一类型来源
**状态：合规**

`Node`、`Edge`、`Graph`、`GraphSummary` 四个结构体完全按照 CLAUDE.md 第四节 Schema 定义实现，字段名、类型、注释一一对应。所有其他模块均通过 `crate::data_manager::` 引用这些类型。

---

## 代码质量问题（严重/中/低）

### [严重] 非测试代码中存在裸 unwrap()

**位置：`src/scanner.rs` 第 160 行**

```rust
(port, result.is_ok() && result.unwrap().is_ok())
```

这段代码在 `probe_host` 函数内（非测试代码）。`result.is_ok()` 为 `true` 时才调用 `result.unwrap()`，逻辑上不会 panic，但这是防御性编程的反模式，且违反 CLAUDE.md 第八节"无裸 `unwrap()` 在非测试代码中"的 STOP 条件。应改为模式匹配：

```rust
(port, matches!(result, Ok(Ok(_))))
```

这是唯一一处非测试代码的裸 `unwrap()`。其余所有 `unwrap()` 均位于 `#[cfg(test)]` 块内，符合规范。

### [中] connection_tracker.rs — is_private 函数对 172.16-31/12 段覆盖不完整

**位置：`src/connection_tracker.rs` 第 277-287 行**

RFC 1918 私有地址段 `172.16.0.0/12` 覆盖 `172.16.x.x` 到 `172.31.x.x`，共 16 个 /16 段。当前实现用字符串前缀匹配，存在以下漏洞：

- `172.20.x.x` ~ `172.29.x.x` 不在任何现有前缀下（`172.2` 能匹配 `172.20-29`，但也会误匹配 `172.2.x.x` 是对的，`172.2` 作为前缀实际上匹配了 `172.20-29`）。
- 仔细检查：`ip.starts_with("172.2")` 会匹配 `172.20.x.x`...`172.29.x.x`，这覆盖了 20-29 段。
- 但 `172.30.x.x` 和 `172.31.x.x` 单独列出了。
- `172.16.x.x`...`172.19.x.x` 由 `172.16.`、`172.17.`、`172.18.`、`172.19.` 单独覆盖。
- 综合来看，实际上覆盖了 16-31 全部段，但实现方式碎片化，不如用 IP 段范围判断清晰，存在维护风险（如未来改动遗漏某段）。建议使用 `ipnet` crate 的 `contains` 方法替换字符串前缀匹配。

### [中] scanner.rs — probe_host 中 Semaphore 使用存在潜在问题

**位置：`src/scanner.rs` 第 154-167 行**

每个端口 task 都 `clone` semaphore 并在 spawn 内部 `acquire()`，但 `_permit` 在 task 结束时 drop，逻辑上是正确的。然而，外层 `scan_subnet` 已经通过 semaphore 限制并发，内层 `probe_host` 又对每个端口 spawn task 并 acquire 同一个 semaphore，导致实际并发限制是针对"端口探测 permit"的，而非"host 探测"的。这与 CLAUDE.md F2 规格"并发数：默认 256 个并发连接"的语义基本吻合，但外层 `scan_subnet` 对 host 的 spawn 并没有受 semaphore 控制（hosts 直接全部 spawn），当子网 /16 时会同时 spawn 65534 个 task，可能引发资源耗尽。

### [低] connection_tracker.rs — Linux 实现 decode_tcp_state 状态码大小写不一致

**位置：`src/connection_tracker.rs` 第 251 行**

状态码 `"0A"` 和 `"0B"` 使用大写十六进制，而 `/proc/net/tcp` 实际输出的状态列可能为小写（如 `0a`）。应做 `to_uppercase()` 转换后再匹配，或同时匹配大小写，否则在真实 Linux 系统上 `"0a"` 会返回 `"UNKNOWN"`。

**位置：`src/connection_tracker.rs` 第 244 行**

```
"0A" => "LISTEN",
"0B" => "CLOSING",
```

测试 fixture `PROC_NET_TCP_FIXTURE` 中使用了 `0A`（大写），但真实 `/proc/net/tcp` 文件通常输出小写 `0a`。这意味着生产代码 `decode_tcp_state` 在真实 Linux 环境下对 LISTEN/CLOSING 状态会返回 "UNKNOWN"。

### [低] visualization.rs — draw_ui 签名与测试不一致

**位置：`src/visualization.rs` 第 365 行 vs 第 946 行**

生产代码 `draw_ui(f: &mut Frame, app: &AppState)` 接受不可变引用 `app`，但测试代码中调用为 `draw_ui(f, &mut app)`（传入可变引用）。在 Rust 中可变引用可以隐式转换为不可变引用，因此不会产生编译错误，但风格不一致，建议统一。

### [低] main.rs — watch 模式缺少 --scan 联动

**位置：`src/main.rs` 第 109-125 行**

watch 模式只刷新连接（`connection_tracker`），不重新执行子网扫描，且注释说"仅演示性刷新"。CLAUDE.md F3 规格要求 `--watch N` 每 N 秒刷新快照，当前实现对连接快照是正确的，但若同时指定 `--scan --watch`，节点列表不会刷新，这与用户期望不符，应在文档或 --help 中明确说明。

---

## 功能合规性

### F1 — 本机网卡扫描
**状态：基本合规**

- 使用 `pnet::datalink::interfaces()` 正确实现。
- macOS/Linux 均有主接口检测逻辑（通过 `route -n get default` / `ip route show default`）。
- 跳过 IPv6（代码注释说明"后续扩展"），暂未违规但 CLAUDE.md 未豁免 IPv6。
- `is_local` 标记逻辑正确（主接口名称匹配）。

### F2 — 局域网设备扫描
**状态：合规**

- 并发 TCP connect，使用 `Semaphore` 限制并发数，默认 256。
- DNS 反向解析使用 `spawn_blocking` 包装。
- 默认端口列表与 CLAUDE.md 规格完全一致（20 个端口）。
- 超时默认 500ms，支持 `--timeout` 参数。

### F3 — TCP/UDP 连接追踪
**状态：合规（macOS/Linux），Windows 占位**

- macOS：调用 `netstat -an -p tcp/udp`，解析文本输出，正确。
- Linux：读取 `/proc/net/tcp`、`/proc/net/tcp6`、`/proc/net/udp`，解析十六进制地址（注意大小写问题见上文）。
- Windows：返回 `bail!` 错误，属于合理占位。
- `--local-only` 和 `--exclude-loopback` 过滤已实现。

### F4 — 拓扑图构建
**状态：合规**

- 使用 `petgraph::DiGraph` 构建有向图。
- 去重合并：`(src, dst, protocol, dst_port)` 为键，累加 `count`，正确。
- 本机节点标记（`is_local`）正确。
- `filter_by_min_connections` 保留 `is_local` 节点，符合规格。

### F5 — JSON 输出
**状态：合规**

- 使用 `serde_json::to_string_pretty`，格式美观。
- 输出 Schema 严格对应 CLAUDE.md 第四节定义（nodes/edges/captured_at/local_ip/summary 五字段）。

### F6 — Graphviz dot 输出
**状态：合规**

- `rankdir=LR` ✓
- 本机节点 `shape=doublecircle color=blue` ✓
- `penwidth` 按连接数计算（`count * 0.5`，最小 1.0，最大 5.0）✓
- TCP 无 style，UDP `style=dashed` ✓
- 节点标签：IP + hostname + ports 数量 ✓

### F7 — ASCII 拓扑图
**状态：合规**

- 本机节点 `[★ IP]` 标识 ✓
- TCP 连接 `──TCP:PORT──►`，UDP 连接 `╌╌UDP:PORT╌╌►` ✓
- 80 列截断（`truncate` 函数）✓
- 标题栏、扫描时间、节点信息、汇总行均已实现 ✓

### F8 — TUI 可视化界面
**状态：基本合规，存在两处功能缺失**

**布局：** 四区域（标题栏/节点列表+连接详情/拓扑图/状态栏）已实现 ✓

**颜色方案：**
- 本机节点 `Color::LightBlue` + `Modifier::BOLD` ✓
- 活跃 TCP/出站连接数值 `Color::Green` ✓（但入站也是 Green，CLAUDE.md 只要求 TCP 用 Green，UDP 用 Yellow；当前代码对入站/出站均用 Green，未区分 TCP vs UDP 颜色——**轻微偏差**）
- UDP 连接数值 `Color::Yellow`：**未实现**（连接详情面板显示的是 inbound/outbound 总数，未区分 TCP/UDP 颜色）
- 无响应节点（ports 为空）`Color::DarkGray` ✓
- 标题栏 `bg=Blue fg=White` ✓
- 状态栏 `bg=Black fg=Green` ✓
- 选中行 `Modifier::REVERSED` ✓

**交互键（CLAUDE.md 规格共 8 个）：**

| 键 | 规格 | 实现状态 |
|---|---|---|
| q | 退出 | ✓ |
| Ctrl+C | 退出 | ✓ |
| r | 立即重新扫描刷新 | 部分实现（仅更新 status_msg，未实际重新扫描） |
| ↑ | 节点列表上移 | ✓ |
| ↓ | 节点列表下移 | ✓ |
| f | 弹出过滤输入框 | ✓ |
| e | 导出 JSON | ✓ |
| Tab | 切换面板焦点 | ✓（切换 focus 字段，但拓扑图面板焦点渲染未区分） |
| ? | 显示帮助覆盖层 | ✓ |

**问题：`r` 键** 仅设置状态消息为"刷新中...（功能需重新扫描，此演示仅刷新界面）"，未实际触发重新扫描。CLAUDE.md 规格要求"立即重新扫描并刷新界面"。这是功能缺失。

**问题：Tab 键**切换了 `app.focus` 字段，但 `draw_ui` 中两个面板的边框样式均未根据 `focus` 字段做高亮区分，焦点切换在视觉上不可见。

---

## 测试质量

### connection_tracker 测试
**状态：优秀**

- 提供了平台无关的测试辅助函数（`test_parse_netstat_output`、`test_parse_proc_net_tcp`），实现了 macOS 和 Linux 两套解析的单元测试。
- macOS 测试：覆盖 ESTABLISHED/LISTEN/UDP/通配符等场景，共约 9 个测试用例。
- Linux 测试：覆盖 IP 解析、状态映射、行计数等场景，共约 6 个测试用例。
- 过滤函数（`is_loopback`、`is_private`、`should_include`）有独立测试。

**注意：** 测试 `test_macos_parse_established_tcp` 断言 ESTABLISHED 数量为 3（包含 127.0.0.1 loopback 连接），但 fixture 中的 `*.8080 LISTEN` 行的 foreign 地址为 `*.*` 会被 `test_split_macos_addr` 返回 `None` 而跳过，因此确实是 3 条。逻辑正确。

### visualization TUI 测试
**状态：合规**

- 使用 `ratatui::backend::TestBackend`，不依赖真实终端。
- 测试了初始渲染、空图渲染、UDP 图渲染、节点过滤、选中节点等场景。
- 共约 8 个 TUI 测试函数。

**不足：**
- `test_tui_key_q_sets_quit_flag` 测试名称误导——实际上没有测试 'q' 键退出，只测试了 `selected` 字段变化和 `filter_mode` 初始值。这属于测试意图与实现不符。
- 没有测试帮助覆盖层渲染（`show_help = true` 路径）。
- 没有测试 export 模式渲染。

### scanner 测试
**状态：良好**

- `parse_ports` 和 `cidr_to_host_ips` 有完整的单元测试，覆盖正常路径、边界值和错误路径。
- 未测试 `scan_interfaces` 和 `probe_host`（需要网络，可接受）。

### graph_builder 测试
**状态：良好**

- 去重、build_graph、filter_by_min_connections 均有测试。
- 覆盖 inbound/outbound 查询。

---

## 必须修复（Blocker）

### B1 — scanner.rs 中存在裸 unwrap() 在非测试代码中

**文件：** `/Users/handsomevictor/Documents/GitHub/netopo/src/scanner.rs:160`

```rust
// 当前代码
(port, result.is_ok() && result.unwrap().is_ok())
```

违反 CLAUDE.md 第八节 STOP 条件："无裸 `unwrap()` 在非测试代码中"。虽然逻辑上不会 panic（先检查了 `is_ok()`），但仍然属于违规的裸 `unwrap()`。

**修复方案：**
```rust
(port, matches!(result, Ok(Ok(_))))
```

### B2 — F8 TUI `r` 键未实现重新扫描

**文件：** `/Users/handsomevictor/Documents/GitHub/netopo/src/visualization.rs:328-331`

CLAUDE.md F8 规格明确要求 `r`：立即重新扫描并刷新界面。当前实现仅修改状态栏文字，不实际执行扫描。

这是 8 个交互键之一，CLAUDE.md 第八节要求"8 个交互键全部可用"才能满足 STOP 条件。当前 `r` 键功能残缺，构成 Blocker。

### B3 — connection_tracker.rs Linux 实现中状态码大小写问题导致 LISTEN/CLOSING 识别失败

**文件：** `/Users/handsomevictor/Documents/GitHub/netopo/src/connection_tracker.rs:251-256`

`decode_tcp_state` 中 `"0A"` 和 `"0B"` 使用大写，但真实 `/proc/net/tcp` 文件输出小写（`0a`、`0b`）。这导致在真实 Linux 系统上 LISTEN 和 CLOSING 状态的连接会被标记为 "UNKNOWN"，影响 F3 功能正确性。

**修复方案：** 在调用前对输入字符串做 `to_uppercase()` 转换，或将匹配模式改为小写。

---

## 建议改进（Non-Blocker）

### N1 — is_private 函数建议使用 ipnet 替代字符串前缀匹配

当前实现碎片化且难以维护，`ipnet` crate 已在依赖中，可使用 `IpNet::contains()` 精确判断 RFC 1918 私有地址。

### N2 — F8 TUI 连接详情面板应区分 TCP（Green）和 UDP（Yellow）颜色

CLAUDE.md 颜色规范：活跃 TCP 连接数值用 `Color::Green`，UDP 连接数值用 `Color::Yellow`。当前连接详情面板只区分"入站/出站"，不区分协议颜色。

### N3 — F8 TUI Tab 键的面板焦点缺乏视觉反馈

`app.focus` 字段被切换，但 `draw_ui` 未根据此字段改变面板边框颜色/样式。建议在当前焦点面板的 `Block` 上设置高亮颜色。

### N4 — watch 模式对 --scan 的行为应在 --help 中明确说明

watch 模式目前只刷新连接，不重新扫描子网节点。如果这是设计决策，应在 CLI 帮助文本或文档中说明。

### N5 — scan_subnet 中 hosts 全量 spawn 可能在大子网 (/16) 下耗尽资源

对 /16 子网会同时 spawn 65534 个 tokio task，每个 task 内再对每个端口 spawn 子 task（最多 20 个），理论上最多约 130 万个并发 task。虽然 tokio 可以处理大量 task，但内存压力显著。建议在外层也用 Semaphore 限制同时进行的 host 探测数量。

### N6 — visualization.rs 中 test_tui_key_q_sets_quit_flag 测试名称误导

该测试实际测试的是 `selected` 字段变化和 `filter_mode` 初始值，与测试名称不符。建议重命名或补充真实的 q 键退出逻辑测试。

---

## 结论：NEEDS_FIX

发现 3 个 Blocker：
- **B1**：非测试代码中存在裸 `unwrap()`（scanner.rs:160）
- **B2**：F8 TUI 的 `r` 键功能未实际实现重新扫描
- **B3**：Linux 连接追踪中 LISTEN/CLOSING 状态因大小写问题识别失败

这 3 个问题阻止项目满足 CLAUDE.md 第八节的 STOP 条件，必须修复后才能进入下一轮审查。
