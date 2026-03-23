# netopo

在终端里探测局域网拓扑、追踪 TCP/UDP 连接、并以 ASCII 图、Graphviz dot 或交互式 TUI 可视化的命令行工具，用 Rust 编写。

---

## 安装

### 从源码编译（推荐）

```bash
git clone https://github.com/handsomevictor/netopo
cd netopo
cargo build --release
# 二进制位于 target/release/netopo
```

### 通过 cargo install

```bash
cargo install --path .
```

安装完成后，`netopo` 命令即可在终端直接使用。

---

## 快速开始

### 1. 扫描本机网卡 + 打印 ASCII 拓扑图

```bash
netopo --scan --ascii
```

输出示例：
```
╔══════════════════════════════════════╗
║           netopo 拓扑图               ║
║  扫描时间: 2026-03-23T10:30:00+08:00  ║
╚══════════════════════════════════════╝

  [★ 192.168.1.100]  mymac.local
         ├──TCP:  443──►  [192.168.1.1]        router.local
         │                └── 开放端口: 80, 443
         └──UDP:   53──►  [8.8.8.8]

  共 3 个节点，2 条连接
```

### 2. 扫描子网设备

```bash
netopo --scan --subnet 192.168.1.0/24
```

扫描指定 CIDR 内所有活跃主机，探测 20 个常用端口，并做 DNS 反向解析。默认并发 256 连接，单连接超时 500ms。

### 3. 实时监控连接（每 10 秒刷新）

```bash
netopo --connections --watch 10 --ascii
```

每 10 秒快照一次本机 TCP/UDP 连接，并重新打印 ASCII 拓扑图。Ctrl+C 退出。

### 4. 导出拓扑为 JSON 文件

```bash
netopo --scan --connections --output-json topology.json
```

生成符合项目 Schema 的 JSON 文件，包含 nodes、edges、captured_at、local_ip、summary 字段。

### 5. 启动 TUI 交互界面

```bash
netopo --scan --connections --tui
```

进入全屏交互界面：左侧节点列表、右侧连接详情、底部 ASCII 拓扑图、底栏快捷键提示。

---

## CLI 参数完整列表

```
USAGE:
    netopo [OPTIONS]

扫描选项:
    --scan                    扫描局域网设备（自动检测主接口子网）
    --subnet <CIDR>           指定扫描网段，如 10.0.0.0/24
    --ports <LIST>            指定端口，如 "22,80,443" 或 "1-1024"
    --concurrency <N>         并发连接数，默认 256
    --timeout <MS>            单连接超时毫秒，默认 500

连接选项:
    --connections             获取当前 TCP/UDP 连接快照
    --local-only              只显示局域网内连接
    --exclude-loopback        排除 loopback 连接
    --watch <SECONDS>         每 N 秒刷新

图构建:
    --graph                   构建拓扑图
    --min-connections <N>     只显示连接数 >= N 的节点

输出格式:
    --output-json <FILE>      输出 JSON 文件
    --output-dot <FILE>       输出 Graphviz dot 文件
    --ascii                   终端打印 ASCII 拓扑图
    --tui                     启动 TUI 交互界面

通用:
    -v, --verbose             详细输出
    -q, --quiet               静默模式
    -h, --help                帮助
    -V, --version             版本
```

### 常用组合

| 场景 | 命令 |
|------|------|
| 快速查看局域网 | `netopo --scan --ascii` |
| 全功能交互界面 | `netopo --scan --connections --tui` |
| 导出拓扑快照 | `netopo --scan --output-json result.json` |
| 持续监控连接 | `netopo --connections --watch 10 --ascii` |
| 指定网段扫描并生成 dot | `netopo --scan --subnet 10.0.0.0/24 --output-dot net.dot` |
| 过滤高频连接节点 | `netopo --connections --min-connections 3 --ascii` |

---

## 默认探测端口

扫描时默认探测以下 20 个端口（可用 `--ports` 覆盖）：

`21, 22, 23, 25, 53, 80, 110, 143, 443, 445, 3306, 3389, 5432, 5900, 6379, 8080, 8443, 8888, 9200, 27017`

---

## 平台支持

| 平台 | 状态 | 连接追踪实现 |
|------|------|-------------|
| macOS | 已支持 | `netstat -an -p tcp/udp` 解析 |
| Linux | 已支持 | 读取 `/proc/net/tcp`、`/proc/net/udp` |
| Windows | 未测试 | 暂不支持（运行时报错提示） |

macOS 和 Linux 均无需 root 权限即可运行基本功能。连接追踪在某些系统上可能需要更高权限；权限不足时程序会给出友好提示而非崩溃。

---

## License

MIT
