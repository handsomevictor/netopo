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

---

## 轮次 2（2026-03-23）

### 问题 5：TUI 焦点管理缺乏视觉区分

**现象：** 初始实现用 `u8` 类型的 `focus` 字段表示当前聚焦的面板（0 = 节点列表，1 = 拓扑图），渲染时所有面板边框颜色相同，用户按 Tab 切换焦点后无任何视觉反馈，无法判断当前操作作用在哪个面板上。

**根因：** 用魔法数字（magic number）表达应用状态，既不自文档化，也无法在渲染层做分支判断，容易遗漏更新逻辑。

**解决方案：** 引入 `FocusedPanel` 枚举（`NodeList` / `TopoGraph`）替代 `u8`。渲染时对每个面板分别计算边框颜色：当前聚焦面板使用 `Color::Yellow`，未聚焦面板使用默认白色，通过 `border_style` 设置到 `Block` 上。

**教训：** TUI 中的用户状态应用类型系统表达，而非魔法数字。枚举不仅提升代码可读性，还能让编译器在 `match` 分支中强制处理所有状态，避免遗漏。

---

### 问题 6：watch 模式循环逻辑写在 main() 函数内

**现象：** watch 模式的 `loop { ... tokio::time::sleep(...).await }` 直接嵌入 `main()` 函数体，导致 `main.rs` 含有超过 50 行的业务流程代码，违反了项目规范中"main.rs 只做参数解析和分发调用，不含业务逻辑"的约定。

**根因：** 在功能原型阶段为求简便，将循环逻辑就地展开，未考虑后续可读性和规范一致性。

**解决方案：** 提取 `run_watch_mode(args, interval)` 异步辅助函数，将循环体移入其中，`main()` 只保留一行调用：`run_watch_mode(&args, interval).await?;`。

**教训：** `main()` 应该只做参数解析和分发调用，业务流程即使简单也应封装为独立函数。这既方便单元测试（可直接测试辅助函数），也让 `main()` 的整体结构一目了然。

---

## 轮次 3（2026-03-23）

### 问题 7：macOS 上活跃 UDP 连接几乎不可见

**现象：** 在 macOS 上运行 `netopo --connections` 时，UDP 连接条目极少甚至为零，即使当前有 DNS 查询或其他 UDP 通信在进行。

**根因：** macOS 内核对 UDP 套接字的状态追踪机制与 Linux 不同。UDP 是无连接协议，macOS 的 `netstat` 只在内核 UDP socket 处于活跃等待状态时才显示条目。DNS 查询（UDP:53）的生命周期极短（几毫秒），查询完成后内核立刻释放 socket，`netstat` 抓取快照时窗口几乎总是空的。相比之下，Linux 的 `/proc/net/udp` 会保留更长时间的 socket 状态信息。

**这是预期行为，不是 bug。** `connection_tracker.rs` 的实现正确解析了 `netstat` 输出，UDP 连接确实存在只是太短暂。

**解决方案（建议）：** 若需要捕获 UDP 流量，可使用 `--watch 1` 以 1 秒间隔持续刷新，提高捕获到短暂 UDP 连接的概率；或考虑后续版本集成 libpcap/BPF 进行实时抓包。当前实现无需修改。

---

### 问题 8：ASCII 输出可读性差——节点堆叠 + 无结构 + 公网 IP 信息稀少

**现象：** 旧版 ASCII 输出将所有连接平铺展示，无法区分局域网设备与公网连接；节点标签只显示裸 IP，无 hostname；端口格式为 `TCP:443(x3)` 不够简洁；没有整体摘要和 Top 端口信息。在真实环境中（数十条连接时），输出一屏无法理解网络结构。

**根因：** 初始实现为"能跑就行"的最小可行版本，直接迭代 edges 列表输出，未对网络结构进行语义分组，也未考虑信息层级。

**解决方案（新格式 v2，当前实现）：**

```
╔══════════════════════════════════════════╗
║              netopo 拓扑图               ║
║             2024-01-15 10:30             ║
╚══════════════════════════════════════════╝

━━━ 局域网设备 (2) ━━━━━━━━━━━━━━━━━━━━

  [★ mymac.local] (192.168.1.100)
  └─► router.local (192.168.1.1)              TCP:443 x3

━━━ 公网连接 (1) ━━━━━━━━━━━━━━━━━━━━━

  Google
  └─► dns.google (8.8.8.8)                   UDP:53

───────────────────────────────────────────────────────────────────────────────
活跃连接: 2  │  TCP: 1  UDP: 1  │  Top端口: 443(x3) 53(x1)
```

关键设计决策：
1. **强制分两组**：`is_lan_ip()` 判断 IPv4 私有段和 IPv6 本地地址（fe80::/fc/fd/::1）为 LAN
2. **公网按 ISP 聚合**：两层识别 — 先用 hostname 关键词 + IP 前缀（10 个主流厂商），再查 MaxMind GeoLite2-ASN 数据库，未知归"其他"
3. **hostname 优先**：`node_display()` 输出 `"hostname (ip)"` 格式
4. **端口格式统一为 `PROTO:PORT x N`**，支持 `--resolve-ports` 翻译为服务名
5. **标题框固定 44 列**：`center_in_box()` 按视觉宽度居中（CJK 字符计为 2 列），时间格式 `"YYYY-MM-DD HH:MM"`
6. **行宽 79 列**：`trunc()` 为 ANSI-aware 截断（跳过 `\x1b[...m` 转义序列计宽）
7. **每 ISP 分组最多显示 10 条**，超出提示 `--all-connections` 参数显示全部

**教训：** CLI 工具的输出设计不亚于代码设计本身。"能跑就行"的输出在真实数据量下毫无用处。应在第一轮就定义清晰的输出结构，而不是在多个 user 反馈后才重构。

