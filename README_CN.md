# netopo

> [English](README.md) | 中文

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey.svg)
![Version](https://img.shields.io/badge/version-0.1.0-green.svg)

> **由 Claude Code 全程构建** — 本项目由多 Agent Claude Code 系统自主开发，零人工代码干预。一组专职 Agent（架构师、开发者、测试、审查、运维、文档、仲裁）迭代构建、测试并持续完善代码库。仲裁 Agent 逐项核查完成标准，并在所有要求满足后通过 hook 自动终止循环。所有 Agent 配置均位于 `.claude/` 目录。

用 Rust 编写的本地网络拓扑探测与可视化工具，在终端运行一条命令，即可清晰呈现局域网设备、活跃 TCP/UDP 连接及完整网络拓扑图——支持 ASCII 图、Graphviz dot、JSON 和交互式 TUI 四种输出格式。

**macOS 和 Linux 无需 root 权限。**

---

## 为什么选择 netopo？

| 功能 | netopo | nmap | ss / netstat | iftop |
|---|:---:|:---:|:---:|:---:|
| 需要 root | ✅ 不需要 | ⚠️ 通常需要 | ✅ 不需要 | ❌ 需要 |
| 局域网设备扫描 | ✅ | ✅ | ❌ | ❌ |
| 实时连接追踪 | ✅ | ❌ | ✅ | ⚠️ 仅带宽 |
| ISP 分组 | ✅ | ❌ | ❌ | ❌ |
| 交互式 TUI | ✅ | ❌ | ❌ | ✅ |
| JSON 导出 | ✅ | ⚠️ 部分支持 | ❌ | ❌ |
| Graphviz 导出 | ✅ | ❌ | ❌ | ❌ |
| 单一静态二进制 | ✅ | ❌ | 系统工具 | ❌ |
| macOS 支持（无需 root）| ✅ | ⚠️ 有限 | ✅ | ❌ |

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

### 扫描局域网并打印 ASCII 拓扑图

```bash
netopo --scan --ascii
```

输出示例：

```
╔══════════════════════════════════════════╗
║           netopo topology                ║
║           2026-03-23 23:03               ║
╚══════════════════════════════════════════╝

━━━ LAN Devices (2) ━━━━━━━━━━━━━━━━━━━━

  [★ mymac.local] (192.168.1.100)
  └─► 192.168.1.1    router.local                   TCP:HTTPS x3

━━━ Internet Connections (2) ━━━━━━━━━━━

  Google
  └─► 8.8.8.8        dns.google                     UDP:53

  AWS
  ├─► 3.33.188.2     ec2-3-33-188-2.compute-1.amaz  TCP:443
  └─► … 5 more, use --all-connections to show all

───────────────────────────────────────────────────────────────────────────────
Active: 8  │  TCP: 6  UDP: 2  │  Top ports: HTTPS(x6) 53(x2)
```

### 启动交互式 TUI

```bash
netopo --scan --connections --tui
```

全屏交互界面，包含节点列表、连接详情面板、ASCII 拓扑视图和键盘快捷键提示。

### 扫描指定子网

```bash
netopo --scan --subnet 192.168.1.0/24
```

扫描指定 CIDR 内所有活跃主机，探测 20 个常用端口，并做 DNS 反向解析。默认 256 并发连接，单连接超时 500ms。

### 实时监控连接（每 10 秒刷新）

```bash
netopo --connections --watch 10 --ascii
```

每 10 秒快照一次本机 TCP/UDP 连接，重新打印 ASCII 拓扑图。Ctrl+C 退出。

### 导出拓扑为 JSON

```bash
netopo --scan --connections --output-json topology.json
```

生成结构化 JSON 文件，包含 `nodes`、`edges`、`captured_at`、`local_ip`、`summary` 字段。

### 过滤 + 端口名解析

```bash
netopo --connections --ascii --filter Google --resolve-ports
```

只显示与 Google 相关的连接，并将端口号翻译为服务名（443 → HTTPS、22 → SSH 等）。

---

## ISP 识别与 IP 归属分类

ASCII 输出将公网连接按 ISP 分组显示，分类采用两层逻辑：

### 第一层：硬编码规则（无需外部文件，即时生效）

通过 hostname 关键词和 IP 前缀识别以下厂商：

| ISP | 识别规则 |
|-----|---------|
| Google | hostname 含 `google`/`1e100`，或 IP 前缀 `8.8.`/`142.250.`/`216.58.` |
| GitHub | hostname 含 `github`，或 IP 前缀 `140.82.`/`185.199.` |
| Apple | hostname 含 `apple`，或 IP 段 `17.x.x.x` |
| Cloudflare | hostname 含 `cloudflare`，或 IP `1.1.1.x`/`1.0.0.x`/`104.18.x`/`172.64.x` |
| AWS | hostname 含 `amazonaws`/`amazon` |
| Akamai | hostname 含 `akamai` |
| Canonical | hostname 含 `canonical`/`ubuntu`，或 IP 前缀 `185.125.` |
| Microsoft | hostname 含 `microsoft`/`azure` |
| Fastly | hostname 含 `fastly` |
| Meta | hostname 含 `facebook`/`instagram`/`meta.` |

匹配不到任何规则的 IP 归入 **其他** 分组。

### 第二层：MaxMind GeoLite2-ASN 数据库（可选，覆盖全量 IP）

若安装了 MaxMind GeoLite2-ASN 数据库，**其他** 分组中的 IP 会自动查询 ASN 组织名，实现全量识别。

**下载/更新数据库：**

```bash
netopo --update-ip-db
```

数据库保存至：`~/.config/netopo/GeoLite2-ASN.mmdb`

> 需要系统已安装 `curl` 且可访问外网。下载完成后立即生效，无需重启。

**数据库来源：** [MaxMind GeoLite2](https://dev.maxmind.com/geoip/geolite2-free-geolocation-data) — 免费，无需注册，每周更新。

---

## CLI 参数参考

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
    --ascii                   终端打印 ASCII 拓扑图（自动检测终端颜色）
    --tui                     启动 TUI 交互界面
    --resolve-ports           端口号转服务名（443→HTTPS, 22→SSH 等）
    --filter <KEYWORD>        按 ISP 名称或 IP 关键词过滤 ASCII 输出
    --all-connections         显示每个 ISP 分组的全部条目（默认每组最多 10 条）

通用:
    --update-ip-db            下载/更新 MaxMind GeoLite2-ASN 数据库
    -v, --verbose             详细输出
    -q, --quiet               静默模式
    -h, --help                帮助
    -V, --version             版本
```

### 常用组合

| 使用场景 | 命令 |
|---------|------|
| 快速查看局域网 | `netopo --scan --ascii` |
| 全功能交互界面 | `netopo --scan --connections --tui` |
| 导出拓扑快照 | `netopo --scan --output-json result.json` |
| 持续监控连接 | `netopo --connections --watch 10 --ascii` |
| 指定网段扫描并生成 dot | `netopo --scan --subnet 10.0.0.0/24 --output-dot net.dot` |
| 过滤高频连接节点 | `netopo --connections --min-connections 3 --ascii` |
| 查看所有 AWS 连接 | `netopo --connections --ascii --filter AWS --all-connections` |
| 带端口名的监控 | `netopo --connections --ascii --resolve-ports` |
| 更新 IP 归属数据库 | `netopo --update-ip-db` |

---

## ASCII 输出说明

### 颜色方案（终端直连时自动启用，管道输出自动关闭）

| 元素 | 颜色 |
|------|------|
| 本机节点 `[★ …]` | 亮蓝粗体 |
| 局域网设备连接行 | 绿色 |
| ISP 分组标签 | 黄色粗体 |
| TCP 端口 | 青色 |
| UDP 端口 | 黄色 |
| 分隔线/框线 | 深灰 |

### 私有地址识别

以下地址段被视为局域网，不出现在公网连接区：

- **IPv4**：`10.x.x.x`、`172.16–31.x.x`、`192.168.x.x`、`127.x.x.x`
- **IPv6**：`::1`（loopback）、`fe80::` 开头（link-local）、`fc`/`fd` 开头（unique local）

---

## 默认探测端口

扫描时默认探测以下 20 个端口（可用 `--ports` 覆盖）：

`21, 22, 23, 25, 53, 80, 110, 143, 443, 445, 3306, 3389, 5432, 5900, 6379, 8080, 8443, 8888, 9200, 27017`

---

## 平台支持

| 平台 | 状态 | 连接追踪实现 |
|------|------|------------|
| macOS | 已支持 | 解析 `netstat -an -p tcp/udp` 输出 |
| Linux | 已支持 | 读取 `/proc/net/tcp`、`/proc/net/udp` |
| Windows | 未测试 | 暂不支持（运行时给出友好错误提示） |

macOS 和 Linux 均无需 root 权限即可运行全部核心功能。连接追踪在某些系统上可能需要更高权限；权限不足时程序给出友好提示而非崩溃。

---

## License

MIT
