# Blockers — 轮次 1

更新日期：2026-03-23

以下问题阻止项目满足 CLAUDE.md 第八节 STOP 条件，必须在下一轮迭代中优先修复。

---

## B1 — 非测试代码中存在裸 unwrap() ✅ FIXED

**严重程度：** BLOCKER（违反 STOP 条件"无裸 unwrap() 在非测试代码中"）

**文件：** `src/scanner.rs` 第 160 行

**违规代码：**
```rust
(port, result.is_ok() && result.unwrap().is_ok())
```

**上下文：** `probe_host` 函数内的 `port_tasks` 生成逻辑（非测试代码）。

**修复方案：**
```rust
(port, matches!(result, Ok(Ok(_))))
```

**修复说明：** 已将 `result.is_ok() && result.unwrap().is_ok()` 替换为 `matches!(result, Ok(Ok(_)))`，消除裸 unwrap()，逻辑等价且无 panic 路径。cargo build + clippy + test 全部通过。

---

## B2 — F8 TUI `r` 键未实现重新扫描 ✅ FIXED

**严重程度：** BLOCKER（违反 STOP 条件"F8：TUI 8 个交互键全部可用"）

**文件：** `src/visualization.rs` 第 328-331 行

**违规代码：**
```rust
KeyCode::Char('r') => {
    app.status_msg =
        "刷新中...（功能需重新扫描，此演示仅刷新界面）".to_string();
}
```

**规格要求（CLAUDE.md F8）：** `r`：立即重新扫描并刷新界面

**修复说明：**
- `AppState` 新增 `local_ip: String` 字段，`run_tui` 签名改为 `run_tui(graph: Graph, local_ip: String)`，`main.rs` 相应更新传参。
- `r` 键处理中先将状态设为"正在刷新..."并立即渲染，然后通过 `tokio::task::block_in_place` + `tokio::runtime::Handle::current().block_on(...)` 在同步 TUI 循环中调用异步函数 `connection_tracker::get_connections(false, false)` 和 `graph_builder::build_graph()`。
- 成功后更新 `app.graph` 为新图数据，状态栏显示"刷新完成 HH:MM:SS"；失败时显示错误信息。

---

## B3 — Linux 连接追踪 decode_tcp_state 大小写问题 ✅ FIXED

**严重程度：** BLOCKER（违反 STOP 条件"F3：能获取 TCP/UDP 连接快照（当前平台）"的正确性）

**文件：** `src/connection_tracker.rs` 第 241-257 行

**问题：** `decode_tcp_state` 函数使用大写十六进制字符串匹配（`"0A"`、`"0B"`），但 Linux `/proc/net/tcp` 实际输出小写（`0a`、`0b`）。导致 LISTEN（`0a`）和 CLOSING（`0b`）状态在真实 Linux 系统上返回 "UNKNOWN"。

**修复说明：** 在 match 前对输入调用 `.to_uppercase()`，即 `match hex.to_uppercase().as_str() { ... }`，使得大小写均可正确匹配。所有现有测试（使用大写 fixture）继续通过。
