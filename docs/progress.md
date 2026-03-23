# Progress

## 当前迭代轮次: 2
## 最后 Arbiter 裁决: NONE

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
| BUG-01 | TUI `r` 刷新键不真正重新扫描，只显示提示信息 | 高 |
| TODO-01 | Windows 平台连接追踪未实现（运行时报错） | 中 |
| TODO-02 | IPv6 地址在扫描和连接追踪中处理不完整 | 低 |
| TODO-03 | `cargo test --all` 尚未在 CI 中验证全部通过 | 高 |
| TODO-04 | `cargo clippy -- -D warnings` 尚未验证零 warning | 高 |

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
