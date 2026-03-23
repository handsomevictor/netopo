# netopo

本地网络拓扑探测工具。扫描局域网设备，追踪连接，可视化网络拓扑图。

> **注意**：项目正在开发中，以下功能列表将随迭代更新。

## 计划功能

- 扫描局域网内设备（IP、hostname、开放端口）
- 发现主机间 TCP/UDP 连接
- 构建拓扑图，支持 JSON / Graphviz dot / ASCII 输出
- TUI 可视化界面
- 跨平台：macOS、Linux、Windows

## 构建

```bash
git clone https://github.com/yourusername/netopo
cd netopo
cargo build --release
```

## 平台支持

| 平台    | 状态     |
|--------|---------|
| Linux  | 开发中   |
| macOS  | 开发中   |
| Windows | 开发中  |

## 许可证

MIT
