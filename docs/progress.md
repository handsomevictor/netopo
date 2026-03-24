# Progress

## Current iteration: 4
## Last arbiter verdict: SHIP_IT (iteration 3 + hotfix, 2026-03-23)

## Module status

| Module | Status | Notes |
|--------|--------|-------|
| cli.rs | Implemented | All CLI parameters defined; port parsing supports comma and range formats |
| data_manager.rs | Implemented | Node/Edge/Graph/GraphSummary type definitions with serde serialization |
| scanner.rs | Implemented | F1 interface enumeration (macOS/Linux), F2 subnet scanning with DNS reverse lookup |
| connection_tracker.rs | Implemented | F3 implemented for macOS (netstat parsing) and Linux (/proc/net parsing) |
| graph_builder.rs | Implemented | F4 petgraph construction, deduplication/merging, min-connections filtering |
| visualization.rs | Implemented | F5 JSON, F6 dot, F7 ASCII, F8 TUI — all output formats complete |
| main.rs | Implemented | Argument dispatch, execution flow, watch mode |

## Open issues

| ID | Description | Priority |
|----|-------------|----------|
| TODO-01 | Windows platform connection tracking not implemented (runtime error) | Medium |

---

## Iteration history

### Iteration 1 (2026-03-23)

**Completed work:**

1. **Project scaffolding:** Initialized Git repository, established `src/` module structure, configured `Cargo.toml` dependencies (clap, serde, tokio, petgraph, pnet, ratatui, crossterm, chrono, anyhow, thiserror, dns-lookup).

2. **Data layer (data_manager.rs):** Implemented `Node`, `Edge`, `Graph`, and `GraphSummary` core types, all with serde serialization/deserialization support.

3. **CLI layer (cli.rs):** Defined all 15 CLI parameters using clap derive macros; implemented `parse_ports()` supporting both comma-separated and range port formats.

4. **Scanner (scanner.rs):** Implemented F1 (`scan_interfaces`, `detect_primary_interface`) and F2 (`scan_subnet`, `probe_host`, `reverse_lookup`), supporting primary interface detection on macOS and Linux; async concurrent scanning with Semaphore rate-limiting.

5. **Connection tracker (connection_tracker.rs):** Implemented F3 — macOS via netstat text parsing, Linux via `/proc/net/tcp` hex address parsing — including `local_only` and `exclude_loopback` filters, with unit tests for both platforms.

6. **Graph builder (graph_builder.rs):** Implemented F4 — `build_graph` with node marking and placeholder creation, `dedup_edges` for deduplication and count merging, `filter_by_min_connections` for node filtering, with unit tests.

7. **Visualization (visualization.rs):** Implemented F5 (JSON output), F6 (dot file generation), F7 (ASCII topology), F8 (ratatui TUI with four-region layout, 7 keyboard shortcuts, color scheme, and help overlay).

8. **Entry point (main.rs):** Assembled all modules, implemented full execution flow and watch mode loop.

9. **Documentation:** Updated README.md, docs/tutorial.md, docs/structure.md, docs/lesson_learned.md, docs/progress.md.

**Known issues at end of iteration:**
- TUI `r` refresh key was a stub; a real rescan requires mpsc channel decoupling
- Windows platform connection tracking not implemented
- Test coverage had not yet reached the 70% per-module target

**Next iteration focus:**
- Run `cargo test --all` and ensure all tests pass
- Run `cargo clippy -- -D warnings` and eliminate all warnings
- Implement real rescan for TUI `r` key
- Add unit tests to reach coverage target

---

### Iteration 2 (2026-03-23)

**Completed work:**

1. **Bug fixes:** Fixed issues NB1–3 (TUI focus with no visual differentiation, watch mode logic in main, Semaphore acquire error handling), documented in `lesson_learned.md` as issues 5–6.

2. **Test additions (cli.rs):** Added 44 new unit tests for cli.rs, covering port parsing edge cases, all CLI flag combinations, and conflict detection. Total test count increased from 93 to 137.

3. **Documentation:** Updated docs/tutorial.md (TUI focus yellow border, connection detail color documentation), docs/lesson_learned.md (added issues 5–6), docs/progress.md (this section).

**Key commit:** `58f06de`

**Known issues at end of iteration:**
- TUI `r` refresh key still a stub
- scanner.rs / connection_tracker.rs test coverage had not reached 70% target

**Next iteration focus:**
- Improve scanner.rs and connection_tracker.rs test coverage
- Update related documentation

---

### Iteration 3 (2026-03-23)

**Completed work:**

1. **CI fix:** Fixed Linux parallel test failure caused by temporary file name collisions (exit 101), introduced `unique_tmp()` helper using AtomicU32 + process ID.
2. **ASCII output redesign:** Completely rebuilt F7 with LAN/internet grouping, ISP aggregation, and bottom summary line (issue 8).
3. **All class-B blocker fixes:** TUI `r` key now triggers a real rescan (using `block_in_place`); fixed Linux `decode_tcp_state` case sensitivity issue; eliminated all bare `unwrap()` calls.

**Key commits:** `c9f7cae`, `60ccb06`

---

### Iteration 4 (2026-03-24)

**Completed work:**

1. **ANSI color output:** Local node bright blue bold, LAN devices green, ISP labels yellow bold, TCP ports cyan, UDP ports yellow, separators dark gray; isatty auto-detection.
2. **MaxMind GeoLite2-ASN integration:** Added `maxminddb` crate; `--update-ip-db` downloads to `~/.config/netopo/GeoLite2-ASN.mmdb`; "Other" group IPs are resolved to ASN organization names.
3. **Hardcoded ISP rule expansion:** Added Cloudflare 104.18/172.64 prefixes; added Canonical, Microsoft, Fastly, Meta.
4. **New CLI parameters:** `--resolve-ports` (port number to service name), `--filter <keyword>` (filter by ISP or IP), `--all-connections` (no per-group limit), `--update-ip-db`.
5. **ISP group limit:** Default max 10 entries per group; overflow prompts `--all-connections`; removed special Akamai aggregation.
6. **Title box redesign:** Fixed at 44 columns; content centered by visual width (CJK = 2 columns); timestamp format `YYYY-MM-DD HH:MM`.
7. **IPv6 local address fix:** `is_lan_ip()` now recognizes `::1`, `fe80::`, `fc`/`fd` prefixes, fixing link-local addresses incorrectly appearing in the internet section.
8. **Empty connection info fix:** `fmt_conns_ex` in the internet section no longer filters by source, displaying all inbound connections.
9. **Documentation sync:** README updated with ISP classification notes, MaxMind DB path and download instructions, complete CLI parameter table; lesson_learned.md updated with ASCII layout fix and design decisions.

**Key commits:** `8b9fcd9`, `037f025`
