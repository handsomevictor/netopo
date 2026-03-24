# Project Structure

> Maintained by doc_agent after each iteration. Reflects the current actual state of the codebase.
> Last updated: Iteration 4 (2026-03-24)

---

## Directory layout

```
netopo/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── cli.rs               # clap parameter definitions
│   ├── data_manager.rs      # Core data type definitions
│   ├── scanner.rs           # F1 + F2: Interface enumeration and subnet scanning
│   ├── connection_tracker.rs # F3: TCP/UDP connection tracking
│   ├── graph_builder.rs     # F4: Topology graph construction
│   └── visualization.rs     # F5 + F6 + F7 + F8: All output formats
├── docs/
│   ├── tutorial.md          # Feature usage tutorial
│   ├── structure.md         # This document
│   ├── lesson_learned.md    # Lessons learned
│   └── progress.md          # Iteration history
├── Cargo.toml
├── CLAUDE.md                # Project master document (highest authority)
└── README.md
```

---

## Module details

### `src/main.rs` — CLI entry point

**Purpose:** Parses command-line arguments, assembles module calls in sequence, and controls execution flow. Contains no business logic.

**Status: Implemented**

Key implementation details:
- Calls `Cli::parse()` to parse arguments; calls `cli.parse_ports()` to parse port lists
- `scanner::local_ip()` retrieves the primary interface IP
- Execution order: `--scan` → `--connections` → `build_graph` → output format handlers
- `--watch` mode is implemented as a `tokio::time::sleep` loop in `run_watch_mode()`
- Prints usage hint when no arguments are provided
- All business logic is delegated to other modules; `main.rs` only orchestrates

---

### `src/cli.rs` — CLI parameter definitions

**Purpose:** Declares all `--flag` options and provides port string parsing.

**Status: Implemented**

Key implementation details:
- Uses `clap` derive macros to define the `Cli` struct
- Parameters are grouped: scan options, connection options, graph options, output formats, general
- `parse_ports()` supports comma-separated format (`"22,80,443"`) and range format (`"1-1024"`), which can be mixed
- Default port list: 21, 22, 23, 25, 53, 80, 110, 143, 443, 445, 3306, 3389, 5432, 5900, 6379, 8080, 8443, 8888, 9200, 27017
- Added in iteration 4: `--resolve-ports`, `--filter <KEYWORD>`, `--all-connections`, `--update-ip-db`

---

### `src/data_manager.rs` — Core data type definitions

**Purpose:** The single source of truth for all data types in the project. Other modules must not define equivalent types.

**Status: Implemented**

Defined types:
- `Node`: ip, hostname, ports, is_local, mac, interface
- `Edge`: src, dst, protocol, src_port, dst_port, state, count
- `Graph`: nodes, edges, captured_at, local_ip, summary
- `GraphSummary`: total_nodes, total_edges, tcp_connections, udp_connections, scan_duration_ms

All types implement `serde::{Serialize, Deserialize}` and can be used directly for JSON output.

---

### `src/scanner.rs` — Interface enumeration and subnet scanning (F1 + F2)

**Purpose:** Enumerates local network interfaces (F1) and concurrently scans subnet hosts and open ports (F2).

**Status: Implemented**

Key implementation details:

**F1 — `scan_interfaces()`:**
- Calls `pnet::datalink::interfaces()` to enumerate all interfaces
- Skips IPv6 addresses (current version handles IPv4 only)
- Calls `detect_primary_interface()` to identify and mark the primary interface

**F1 — `detect_primary_interface()`:**
- macOS: runs `route -n get default` and extracts the `interface:` line
- Linux: runs `ip route show default` and extracts the `dev <name>` token

**F2 — `scan_subnet()`:**
- Parses CIDR to obtain all host IPs
- Spawns `tokio::spawn` tasks per IP; uses a `Semaphore` to cap concurrency
- Concurrently probes all specified ports per IP via TCP connect; records open ports
- Calls `reverse_lookup()` for DNS reverse resolution when open ports are found
- Hosts with no open ports are not added as nodes

**Helper functions:**
- `default_cidr()`: derives the subnet CIDR from the primary interface IP using pnet's IpNet
- `local_ip()`: returns the primary interface's IPv4 address as a string

---

### `src/connection_tracker.rs` — TCP/UDP connection tracking (F3)

**Purpose:** Cross-platform snapshot of all active TCP/UDP connections on the local host.

**Status: Implemented (macOS + Linux)**

Key implementation details:

**macOS implementation:**
- Runs `netstat -an -p tcp` and `netstat -an -p udp` to capture output
- `parse_netstat_line()` parses text output, supporting both IPv4 (`a.b.c.d.port`) and IPv6 (`[addr].port`) formats
- Wildcard entries (`*.*`) are skipped

**Linux implementation:**
- Reads `/proc/net/tcp`, `/proc/net/tcp6`, and `/proc/net/udp`
- `parse_hex_addr()` decodes hexadecimal addresses with little-endian byte ordering
- `decode_tcp_state()` translates hex state codes to readable strings (e.g., `"01"` → `"ESTABLISHED"`)

**Filtering logic:**
- `--local-only`: retains only connections where the destination is a private address
- `--exclude-loopback`: drops connections where source or destination is `127.x.x.x` or `::1`

**Windows:** Returns a `bail!` error at runtime with a message indicating the platform is not yet supported.

**Tests:** Unit tests cover both macOS and Linux parsing, including normal cases and wildcard-skipping behavior.

---

### `src/graph_builder.rs` — Topology graph construction (F4)

**Purpose:** Constructs a petgraph directed graph from Node and Edge lists, deduplicates and merges edges, and marks the local node.

**Status: Implemented**

Key implementation details:

**`build_graph()`:**
- Runs `dedup_edges()` on `edges`: connections with the same (src, dst, protocol, dst_port) accumulate `count`
- Iterates `nodes` to mark `is_local` (nodes whose IP matches `local_ip`)
- Adds placeholder nodes (empty `ports`) for IPs in deduplicated edges that have no corresponding `Node`
- Constructs a `petgraph::graph::DiGraph<String, String>` with edge labels in the form `"TCP x3"`

**`dedup_edges()`:**
- Uses a `HashMap<(src, dst, protocol, dst_port), Edge>` for deduplication and count accumulation
- Results are sorted by (src, dst, protocol, dst_port) for stable output

**`filter_by_min_connections()`:**
- Collects the set of IPs involved in connections where `count >= min_connections`
- Retains the local node and nodes in that set; removes all others along with non-qualifying edges
- Updates `total_nodes` and `total_edges` in `summary`

---

### `src/visualization.rs` — Output formats (F5 + F6 + F7 + F8)

**Purpose:** Implements all four output formats.

**Status: Implemented**

**F5 — `output_json()`:**
- Serializes `Graph` using `serde_json::to_string_pretty()`
- Writes to the specified file; returns a contextual error on failure

**F6 — `output_dot()`:**
- Manually assembles the dot string, generating each node and edge in sequence
- Local node: `shape=doublecircle color=blue`
- Edge penwidth = `max(1.0, count * 0.5).min(5.0)`
- UDP edges include `style=dashed`
- Node labels: IP + hostname + `N ports`, separated by `\n`

**F7 — `print_ascii(graph, &AsciiOptions)`:**
- Title box is fixed at 44 columns; content is centered by visual width (`visual_width()` counts CJK characters as 2 columns); timestamp format is `YYYY-MM-DD HH:MM`
- Internet connections are grouped by ISP (two-layer detection: hardcoded rules + MaxMind GeoLite2-ASN DB)
- IPv4 private ranges and IPv6 local addresses (`fe80::`/`fc`/`fd`/`::1`) are classified as LAN
- Each ISP group shows at most 10 entries by default; overflow is indicated with a prompt to use `--all-connections`
- ANSI colors (auto-detected via isatty, disabled when piped): local node bright blue bold, LAN green, ISP label yellow bold, TCP cyan, UDP yellow, separators dark gray
- `trunc()` is ANSI-aware truncation (skips escape sequences when measuring width) to guarantee output fits within 79 columns
- Supports `--resolve-ports` (port number to service name) and `--filter <keyword>` (filter by ISP or IP)

**F8 — `run_tui()` / `run_tui_loop()` / `draw_ui()`:**
- Four-region layout: title bar (1 row) + middle area (node list + connection details) + topology view (4 rows) + status bar (1 row)
- Node list: left 40%, with selection highlight and color levels (local node blue, nodes without ports gray)
- Connection details: right 60%, shows inbound/outbound counts for the selected node (green), open ports, and hostname
- Topology view: simplified ASCII connection graph from the local node to outbound targets
- Status bar: shows keyboard shortcuts by default; shows input prompts in filter/export mode
- All 7 keyboard shortcuts implemented: q, Ctrl+C, r, up/down arrows, f, e, Tab, ?

---

## TODO / Known limitations

| Module | Issue | Priority |
|--------|-------|----------|
| connection_tracker.rs | Windows platform not implemented | Medium |
| visualization.rs | TUI topology panel shows only local outbound connections, not the full network graph | Low |
| scanner.rs | IPv6 addresses are skipped and not included in subnet scanning | Low |
