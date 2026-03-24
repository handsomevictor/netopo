# netopo

> English | [中文](README_CN.md)

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)
![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey.svg)
![Version](https://img.shields.io/badge/version-0.1.0-green.svg)

> **Built with Claude Code** — This project was developed end-to-end by a multi-agent Claude Code system operating autonomously. A team of specialized agents (architect, developer, tester, reviewer, devops, doc, arbiter) iteratively built, tested, and refined the codebase with zero manual code edits. The arbiter agent evaluated completion criteria and halted the loop via a hook when all requirements were satisfied. All agent configuration lives in the `.claude/` directory.

A terminal-based local network topology discovery and visualization tool written in Rust. Run one command and get a clear picture of your LAN devices, active TCP/UDP connections, and the full network graph — rendered as ASCII art, Graphviz dot, JSON, or an interactive TUI.

No root required on macOS and Linux.

---

## Why netopo?

| Feature | netopo | nmap | ss / netstat | iftop |
|---|:---:|:---:|:---:|:---:|
| Root required | ✅ Not needed | ⚠️ Often | ✅ Not needed | ❌ Required |
| LAN device scan | ✅ | ✅ | ❌ | ❌ |
| Live connection tracking | ✅ | ❌ | ✅ | ⚠️ Bandwidth only |
| ISP grouping | ✅ | ❌ | ❌ | ❌ |
| Interactive TUI | ✅ | ❌ | ❌ | ✅ |
| JSON export | ✅ | ⚠️ Partial | ❌ | ❌ |
| Graphviz export | ✅ | ❌ | ❌ | ❌ |
| Single static binary | ✅ | ❌ | System tool | ❌ |
| macOS support (no root) | ✅ | ⚠️ Limited | ✅ | ❌ |

---

## Installation

### Build from source (recommended)

```bash
git clone https://github.com/handsomevictor/netopo
cd netopo
cargo build --release
# Binary is at target/release/netopo
```

### Via cargo install

```bash
cargo install --path .
```

Once installed, the `netopo` command is available in your terminal.

---

## Quick Start

### Scan LAN and print ASCII topology

```bash
netopo --scan --ascii
```

Example output:

```
╔══════════════════════════════════════════╗
║           netopo topology                ║
║           2026-03-23 23:03               ║
╚══════════════════════════════════════════╝

━━━ LAN Devices (2) ━━━━━━━━━━━━━━━━━━━━

  [★ mymac.local] (192.168.1.100)
  └─► router.local (192.168.1.1)              TCP:HTTPS x3

━━━ Internet Connections (2) ━━━━━━━━━━━

  Google
  └─► dns.google (8.8.8.8)                   UDP:53

  AWS
  ├─► ec2-3-33-188-2.compute-1.amazonaws.com (3.33.188.2)  TCP:443
  └─► … 5 more, use --all-connections to show all

───────────────────────────────────────────────────────────────────────────────
Active: 8  │  TCP: 6  UDP: 2  │  Top ports: HTTPS(x6) 53(x2)
```

### Launch the interactive TUI

```bash
netopo --scan --connections --tui
```

Full-screen interactive interface with node list, connection details panel, ASCII topology view, and keyboard shortcuts.

### Scan a specific subnet

```bash
netopo --scan --subnet 192.168.1.0/24
```

Scans all hosts in the given CIDR, probes 20 common ports, and performs reverse DNS lookups. Default: 256 concurrent connections, 500ms timeout per connection.

### Monitor connections live (refresh every 10 seconds)

```bash
netopo --connections --watch 10 --ascii
```

Snapshots all TCP/UDP connections every 10 seconds and reprints the ASCII topology. Press Ctrl+C to exit.

### Export topology to JSON

```bash
netopo --scan --connections --output-json topology.json
```

Produces a structured JSON file with `nodes`, `edges`, `captured_at`, `local_ip`, and `summary` fields.

### Filter and resolve port names

```bash
netopo --connections --ascii --filter Google --resolve-ports
```

Shows only connections related to Google, with port numbers translated to service names (443 → HTTPS, 22 → SSH, etc.).

---

## ISP Identification and IP Classification

The ASCII output groups internet connections by ISP. Classification uses two layers:

### Layer 1: Hardcoded rules (no external files, instant)

Vendors are identified by hostname keywords and IP prefixes:

| ISP | Detection rules |
|-----|----------------|
| Google | hostname contains `google`/`1e100`, or IP prefix `8.8.`/`142.250.`/`216.58.` |
| GitHub | hostname contains `github`, or IP prefix `140.82.`/`185.199.` |
| Apple | hostname contains `apple`, or IP range `17.x.x.x` |
| Cloudflare | hostname contains `cloudflare`, or IP `1.1.1.x`/`1.0.0.x`/`104.18.x`/`172.64.x` |
| AWS | hostname contains `amazonaws`/`amazon` |
| Akamai | hostname contains `akamai` |
| Canonical | hostname contains `canonical`/`ubuntu`, or IP prefix `185.125.` |
| Microsoft | hostname contains `microsoft`/`azure` |
| Fastly | hostname contains `fastly` |
| Meta | hostname contains `facebook`/`instagram`/`meta.` |

IPs that match no rule are placed in the **Other** group.

### Layer 2: MaxMind GeoLite2-ASN database (optional, full IP coverage)

If the MaxMind GeoLite2-ASN database is installed, IPs in the **Other** group are automatically resolved to their ASN organization name.

**Download or update the database:**

```bash
netopo --update-ip-db
```

The database is saved to `~/.config/netopo/GeoLite2-ASN.mmdb`.

> Requires `curl` and internet access. Takes effect immediately after download, no restart needed.

**Source:** [MaxMind GeoLite2](https://dev.maxmind.com/geoip/geolite2-free-geolocation-data) — free, no registration required, updated weekly.

---

## CLI Reference

```
USAGE:
    netopo [OPTIONS]

Scan options:
    --scan                    Scan LAN devices (auto-detects primary interface subnet)
    --subnet <CIDR>           Specify subnet, e.g. 10.0.0.0/24
    --ports <LIST>            Specify ports, e.g. "22,80,443" or "1-1024"
    --concurrency <N>         Concurrent connections, default 256
    --timeout <MS>            Per-connection timeout in ms, default 500

Connection options:
    --connections             Snapshot current TCP/UDP connections
    --local-only              Show only LAN connections
    --exclude-loopback        Exclude loopback connections
    --watch <SECONDS>         Refresh every N seconds

Graph options:
    --graph                   Build topology graph
    --min-connections <N>     Only show nodes with >= N connections

Output formats:
    --output-json <FILE>      Write JSON file
    --output-dot <FILE>       Write Graphviz dot file
    --ascii                   Print ASCII topology to terminal (auto-detects color support)
    --tui                     Launch interactive TUI
    --resolve-ports           Translate port numbers to service names (443→HTTPS, 22→SSH, etc.)
    --filter <KEYWORD>        Filter ASCII output by ISP name or IP keyword
    --all-connections         Show all entries per ISP group (default: max 10 per group)

General:
    --update-ip-db            Download/update MaxMind GeoLite2-ASN database
    -v, --verbose             Verbose output
    -q, --quiet               Quiet mode
    -h, --help                Show help
    -V, --version             Show version
```

### Common combinations

| Use case | Command |
|----------|---------|
| Quick LAN overview | `netopo --scan --ascii` |
| Full interactive interface | `netopo --scan --connections --tui` |
| Export topology snapshot | `netopo --scan --output-json result.json` |
| Live connection monitor | `netopo --connections --watch 10 --ascii` |
| Scan subnet, export dot | `netopo --scan --subnet 10.0.0.0/24 --output-dot net.dot` |
| Filter high-traffic nodes | `netopo --connections --min-connections 3 --ascii` |
| Show all AWS connections | `netopo --connections --ascii --filter AWS --all-connections` |
| Monitor with port names | `netopo --connections --ascii --resolve-ports` |
| Update IP database | `netopo --update-ip-db` |

---

## ASCII Output Details

### Color scheme (auto-enabled when stdout is a terminal, disabled when piped)

| Element | Color |
|---------|-------|
| Local node `[★ …]` | Bright blue, bold |
| LAN device connection lines | Green |
| ISP group labels | Yellow, bold |
| TCP ports | Cyan |
| UDP ports | Yellow |
| Separator lines / borders | Dark gray |

### Private address detection

The following ranges are treated as LAN and excluded from the internet connections section:

- **IPv4**: `10.x.x.x`, `172.16–31.x.x`, `192.168.x.x`, `127.x.x.x`
- **IPv6**: `::1` (loopback), `fe80::` (link-local), `fc`/`fd` prefix (unique local)

---

## Default probe ports

When scanning, these 20 ports are probed by default (override with `--ports`):

`21, 22, 23, 25, 53, 80, 110, 143, 443, 445, 3306, 3389, 5432, 5900, 6379, 8080, 8443, 8888, 9200, 27017`

---

## Platform support

| Platform | Status | Connection tracking |
|----------|--------|---------------------|
| macOS | Supported | Parses `netstat -an -p tcp/udp` output |
| Linux | Supported | Reads `/proc/net/tcp`, `/proc/net/udp` |
| Windows | Not tested | Not supported (runtime error with helpful message) |

Both macOS and Linux run without root for all core features. Connection tracking may require elevated permissions on some systems; when permissions are insufficient, the tool prints a friendly message rather than crashing.

---

## License

MIT
