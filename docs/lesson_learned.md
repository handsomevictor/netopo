# 经验教训

> 本文档由 doc_agent 维护，记录每轮迭代中遇到的真实问题和解决方案。
> 每轮追加，不覆盖历史。

---

## 轮次 1（2026-03-23）

### 问题 1：macOS `netstat` 输出中地址和端口之间没有冒号分隔

**现象：** macOS 的 `netstat -an -p tcp` 输出格式为 `192.168.1.100.54321`，用最后一个点分隔 IP 和端口，而不是 `192.168.1.100:54321`。直接用 `split(':')` 无法解析，导致所有连接被丢弃。

**根因：** macOS BSD netstat 沿用了历史传统格式，用最后一段作为端口号，与 Linux 的 `IP:PORT` 格式完全不同。

**解决方案：** 在 `connection_tracker.rs` 的 `split_addr_port()` 函数中使用 `rsplitn(2, '.')` 从右侧分割，取最后一段为端口号，前面的部分整体作为 IP 地址。IPv6 格式 `[::1].80` 则单独处理，先找 `]` 的位置再截取端口。

---

### 问题 2：Linux `/proc/net/tcp` 地址是 little-endian 十六进制，字节序与直觉相反

**现象：** `/proc/net/tcp` 中 `192.168.1.1:80` 对应的字段为 `0101A8C0:0050`，如果直接按大端序解析会得到完全错误的 IP 地址。

**根因：** Linux 内核在 `/proc/net/tcp` 中以 little-endian 32 位整数存储 IPv4 地址（即主机字节序）。`0x0101A8C0` 实际上是 `[0x01, 0x01, 0xA8, 0xC0]` 按小端排列，对应 `192.168.1.1`，需要逐字节反转解读。

**解决方案：** `parse_hex_addr()` 中对 8 位十六进制字符串（IPv4）解析为 `u32` 后，按字节分别取出 `raw & 0xFF`、`raw >> 8 & 0xFF` 等，按 little-endian 顺序拼接 IP 字符串。对 IPv6 的 32 位十六进制字节块则用 `u32::swap_bytes()` 翻转后再格式化。

---

### 问题 3：TUI 拓扑图面板的 `r` 刷新键无法真正重新扫描

**现象：** 用户在 TUI 中按 `r` 期望重新扫描网络、刷新节点和连接数据，但当前实现只更新了一条状态栏消息，节点列表和连接数据保持初始值不变。

**根因：** `run_tui_loop` 持有已构建好的 `AppState`（包含初始 `Graph`），TUI 事件循环是单线程同步的，而重新扫描（`scan_subnet` + `get_connections`）是 async 操作，无法直接在事件循环里 `await`。在首轮实现时选择了简化方案，将重新扫描推迟处理。

**解决方案（已知 TODO）：** 后续需要引入 `tokio::sync::mpsc` 通道：主 TUI 线程按 `r` 时向后台 task 发送扫描请求，后台 task 完成后将新的 `Graph` 通过通道发回，TUI 线程在 `event::poll` 超时时检查通道，收到新数据后更新 `AppState`。当前轮次未实现，已记录在 `docs/structure.md` 的 TODO 表中。

---

### 问题 4：`scan_subnet` 的 `Semaphore` 在高并发下的 acquire 错误处理

**现象：** 在 `probe_host` 中调用 `sem.acquire().await`，返回值是 `Result<SemaphorePermit, AcquireError>`。初版代码用 `let _permit = sem.acquire().await;` 忽略了 `Result`，当 `Semaphore` 被关闭时（理论上不会，但 clippy 会警告）会 silent panic。

**根因：** `tokio::sync::Semaphore::acquire()` 返回 `Result`，在 semaphore 尚未关闭时总是 `Ok`，但 Rust 要求显式处理 `Result`，否则编译器警告。

**解决方案：** 使用 `let _permit = sem.acquire().await;` 的隐式丢弃是安全的（因为此处 Semaphore 不会被关闭），但为消除 clippy 的 `must_use` 提示，可改写为 `let Ok(_permit) = sem.acquire().await else { return (port, false); };`，或在任务里用 `sem.acquire_owned().await.ok()` 配合 `?` 传播。

