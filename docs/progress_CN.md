# Progress

## 当前迭代轮次: 3
## 最后 Arbiter 裁决: SHIP_IT（轮次3+hotfix，2026-03-23）

## 模块状态

| 模块 | 状态 | 说明 |
|------|------|------|
| cli.rs | 已实现 | 全部 CLI 参数定义完成，端口解析支持逗号和范围格式 |
| data_manager.rs | 已实现 | Node/Edge/Graph/GraphSummary 类型定义，serde 序列化 |
| scanner.rs | 已实现 | F1 网卡枚举（macOS/Linux）、F2 子网扫描含 DNS 反解析 |
| connection_tracker.rs | 已实现 | F3 macOS（netstat 解析）和 Linux（/proc/net 解析）均实现 |
| graph_builder.rs | 已实现 | F4 petgraph 构建、去重合并、min-connections 过滤 |
| visualization.rs | 已实现 | F5 JSON、F6 dot、F7 ASCII、F8 TUI 全部实现 |
| main.rs | 已实现 | 参数组装、执行流程、watch 模式 |

## 待解决问题

| 编号 | 描述 | 优先级 |
|------|------|--------|
| TODO-01 | Windows 平台连接追踪未实现（运行时报错） | 中 |

---

## 迭代历史

### 轮次 1（2026-03-23）

**完成工作：**

1. **项目脚手架搭建：** 初始化 Git 仓库，建立 `src/` 模块结构，配置 `Cargo.toml` 依赖（clap、serde、tokio、petgraph、pnet、ratatui、crossterm、chrono、anyhow、thiserror、dns-lookup）。

2. **数据层（data_manager.rs）：** 实现 `Node`、`Edge`、`Graph`、`GraphSummary` 四个核心类型，全部支持 serde 序列化/反序列化。

3. **CLI 层（cli.rs）：** 用 clap derive 宏定义全部 15 个 CLI 参数，实现 `parse_ports()` 支持逗号和范围两种端口格式。

4. **扫描层（scanner.rs）：** 实现 F1（`scan_interfaces`、`detect_primary_interface`）和 F2（`scan_subnet`、`probe_host`、`reverse_lookup`），支持 macOS 和 Linux 主接口检测，async 并发扫描配 Semaphore 限流。

5. **连接追踪（connection_tracker.rs）：** 实现 F3，macOS 通过 netstat 文本解析，Linux 通过 `/proc/net/tcp` 十六进制地址解析，包含 `local_only` 和 `exclude_loopback` 过滤逻辑，附带两平台单元测试。

6. **图构建（graph_builder.rs）：** 实现 F4，`build_graph` 做节点标记和占位补全，`dedup_edges` 去重合并，`filter_by_min_connections` 节点过滤，包含单元测试。

7. **可视化（visualization.rs）：** 实现 F5（JSON 输出）、F6（dot 文件生成）、F7（ASCII 拓扑图）、F8（ratatui TUI，含四区域布局、7 个交互键、颜色方案、帮助覆盖层）。

8. **入口（main.rs）：** 组装各模块，实现完整执行流程和 watch 模式循环。

9. **文档：** 更新 README.md、docs/tutorial.md、docs/structure.md、docs/lesson_learned.md、docs/progress.md（本文件）。

**已知遗留问题：**
- TUI `r` 刷新键为 stub 实现，后续需用 mpsc 通道解耦扫描和渲染
- Windows 平台连接追踪待实现
- 测试覆盖率尚未达到各模块 70% 的目标

**下一轮重点：**
- 运行 `cargo test --all` 验证全部通过
- 运行 `cargo clippy -- -D warnings` 消除所有 warning
- 实现 TUI `r` 键的真正重新扫描
- 补充各模块单元测试至覆盖率要求

---

### 轮次 2（2026-03-23）

**完成工作：**

1. **缺陷修复：** 修复 NB1-3（TUI 焦点无视觉区分、watch 模式逻辑混入 main、Semaphore acquire 错误处理），对应 `lesson_learned.md` 问题 5–6。

2. **测试补充（cli.rs）：** 新增 44 个 cli.rs 单元测试，涵盖端口解析边界情况、全部 CLI 标志组合及冲突检测，测试总数从 93 增至 137。

3. **文档：** 更新 docs/tutorial.md（TUI 焦点黄色边框说明、连接详情颜色说明）、docs/lesson_learned.md（新增问题 5–6）、docs/progress.md（本节）。

**关键 commit：** `58f06de`

**已知遗留问题：**
- TUI `r` 刷新键仍为 stub 实现
- scanner.rs / connection_tracker.rs 测试覆盖率未达 70% 目标

**下一轮重点：**
- 提升 scanner.rs 和 connection_tracker.rs 测试覆盖率
- 更新相关文档

---

### 轮次 3（2026-03-23）

**完成工作：**

1. **CI 修复：** 修复 Linux 并行测试中临时文件名冲突导致的 exit 101，引入 `unique_tmp()` 辅助函数（AtomicU32 + 进程 ID）。
2. **ASCII 输出重设计：** 完整重构 F7，局域网/公网分组、ISP 聚合、底部摘要（问题 8）。
3. **全部 B 级 blocker 修复：** TUI `r` 刷新键真正重新扫描（block_in_place）、Linux decode_tcp_state 大小写问题、裸 unwrap 消除。

**关键 commits：** `c9f7cae`、`60ccb06`

---

### 轮次 4（2026-03-24）

**完成工作：**

1. **ANSI 颜色输出：** 本机节点亮蓝粗体、LAN 设备绿色、ISP 标签黄色粗体、TCP 端口青色、UDP 端口黄色、分隔符深灰；isatty 自动检测。
2. **MaxMind GeoLite2-ASN 集成：** 添加 `maxminddb` crate，`--update-ip-db` 下载到 `~/.config/netopo/GeoLite2-ASN.mmdb`，对"其他"分组 IP 做 ASN 组织名查询。
3. **硬编码 ISP 规则扩展：** Cloudflare 补充 104.18/172.64 前缀；新增 Canonical/Microsoft/Fastly/Meta。
4. **新 CLI 参数：** `--resolve-ports`（端口号→服务名）、`--filter <keyword>`（按 ISP/IP 过滤）、`--all-connections`（不限每组条数）、`--update-ip-db`。
5. **ISP 分组上限：** 默认每组最多 10 条，超出提示 `--all-connections`，不再对 Akamai 特殊聚合。
6. **标题框重设计：** 固定 44 列，内容按视觉宽度居中（CJK 2 列），时间格式 `YYYY-MM-DD HH:MM`。
7. **IPv6 本地地址修复：** `is_lan_ip()` 新增 `::1`、`fe80::`、`fc/fd` 前缀识别，修复 link-local 地址误入公网区的问题。
8. **空连接信息修复：** 公网连接区 `fmt_conns_ex` 改为不过滤 src，显示所有入向连接。
9. **文档同步：** README 新增 ISP 分类说明、MaxMind DB 路径和下载方式、完整 CLI 参数表；lesson_learned.md 修复 ASCII 图对齐问题并更新设计决策。

**关键 commits：** `8b9fcd9`、`037f025`
