# Lessons Learned

> Maintained by doc_agent. Records real problems encountered during each iteration and their solutions.
> Appended each iteration; historical entries are never overwritten.

---

## Iteration 1 (2026-03-23)

### Issue 1: macOS `netstat` separates address and port with a dot, not a colon

**Problem:** The output of `netstat -an -p tcp` on macOS uses the format `192.168.1.100.54321`, where the last dot separates the IP from the port. Using `split(':')` to parse the address discards all connections.

**Root cause:** macOS BSD netstat follows a historical format that uses the last segment as the port number, which is entirely different from the Linux `IP:PORT` format.

**Solution:** In `connection_tracker.rs`, the `split_addr_port()` function uses `rsplitn(2, '.')` to split from the right, treating the last segment as the port and everything before it as the IP. The IPv6 format `[::1].80` is handled separately by locating the `]` character and slicing off the port.

**Takeaway:** Always verify the exact output format of system tools on each target platform before parsing. What works on Linux may not work on macOS, and vice versa.

---

### Issue 2: Linux `/proc/net/tcp` stores addresses in little-endian hex — the byte order is counter-intuitive

**Problem:** In `/proc/net/tcp`, `192.168.1.1:80` appears as `0101A8C0:0050`. Parsing this naively as big-endian produces a completely wrong IP address.

**Root cause:** The Linux kernel stores IPv4 addresses in `/proc/net/tcp` as little-endian 32-bit integers (host byte order). `0x0101A8C0` is the bytes `[0x01, 0x01, 0xA8, 0xC0]` in little-endian order, which corresponds to `192.168.1.1` after byte-reversal.

**Solution:** In `parse_hex_addr()`, the 8-character hex string (IPv4) is parsed as a `u32`, then each byte is extracted with `raw & 0xFF`, `raw >> 8 & 0xFF`, etc., and assembled in little-endian order to form the IP string. For the 32-bit hex blocks in IPv6 entries, `u32::swap_bytes()` is used before formatting.

**Takeaway:** When reading kernel data structures, always check the byte order documented in the kernel source or `/proc` documentation. Do not assume network byte order.

---

### Issue 3: The TUI `r` refresh key did not trigger a real rescan

**Problem:** Pressing `r` in the TUI was expected to rescan the network and refresh node and connection data, but the initial implementation only updated a status bar message. The node list and connection data remained unchanged.

**Root cause:** `run_tui_loop` held an already-constructed `AppState` (with the initial `Graph`). The TUI event loop is synchronous, while rescanning (`scan_subnet` + `get_connections`) requires async operations — `await` cannot be called directly inside the event loop. The first iteration chose a simplified approach and deferred the real rescan.

**Solution (applied in iteration 3):** Used `tokio::task::block_in_place` to run the async rescan synchronously inside the event loop. This allows the TUI thread to block briefly while the scan completes, then update `AppState` with the new `Graph`. A longer-term approach would use `tokio::sync::mpsc` channels to decouple scanning from rendering entirely.

**Takeaway:** TUI event loops and async runtimes do not mix naturally. Plan the threading model before implementing refresh functionality.

---

### Issue 4: `Semaphore::acquire()` in `scan_subnet` silently discarded the `Result`

**Problem:** The initial code used `let _permit = sem.acquire().await;` which ignores the `Result<SemaphorePermit, AcquireError>`. While a non-closed semaphore always returns `Ok`, Clippy flags this as a `must_use` warning, and silent errors could occur if the semaphore were ever closed.

**Root cause:** `tokio::sync::Semaphore::acquire()` returns a `Result`, and Rust requires explicit handling of `Result` to avoid compiler warnings.

**Solution:** Rewrote the call as `let Ok(_permit) = sem.acquire().await else { return (port, false); };`, making the error path explicit and eliminating the Clippy warning.

**Takeaway:** Never silently discard `Result` values, even in cases that are theoretically safe. The explicit handling serves as documentation and guards against future changes to the semaphore's lifecycle.

---

## Iteration 2 (2026-03-23)

### Issue 5: TUI focus state had no visual differentiation

**Problem:** The initial implementation used a `u8` field named `focus` to represent the currently focused panel (0 = node list, 1 = topology graph). All panel borders rendered in the same color. After pressing Tab to switch focus, users had no visual feedback indicating which panel was active.

**Root cause:** Using magic numbers to represent application state is neither self-documenting nor easy to use for branching logic in the renderer, making it easy to miss update paths.

**Solution:** Replaced the `u8` with a `FocusedPanel` enum (`NodeList` / `TopoGraph`). At render time, each panel's border color is computed separately: the focused panel uses `Color::Yellow`, while unfocused panels use the default color, applied via `border_style` on the `Block`.

**Takeaway:** In TUI applications, represent user-visible state with the type system rather than magic numbers. Enums improve readability and allow the compiler to enforce exhaustive handling in `match` branches.

---

### Issue 6: Watch mode loop logic was embedded in `main()`

**Problem:** The `loop { ... tokio::time::sleep(...).await }` for watch mode was written directly inside `main()`, resulting in over 50 lines of business flow logic in `main.rs` — violating the project's rule that `main.rs` may only parse arguments and dispatch calls.

**Root cause:** During rapid prototyping, the loop was expanded inline for simplicity without considering long-term readability or consistency with project conventions.

**Solution:** Extracted a `run_watch_mode(args, interval)` async helper function and moved the loop body into it. `main()` retains a single dispatch call: `run_watch_mode(&args, interval).await?;`.

**Takeaway:** `main()` should only parse arguments and dispatch calls — even simple business flows should be encapsulated in dedicated functions. This makes `main()` readable at a glance and makes the helper functions independently testable.

---

## Iteration 3 (2026-03-23)

### Issue 7: Active UDP connections are nearly invisible on macOS

**Problem:** Running `netopo --connections` on macOS yields very few or zero UDP connection entries, even when DNS queries or other UDP traffic is actively occurring.

**Root cause:** macOS kernel UDP socket tracking differs from Linux. UDP is connectionless; macOS `netstat` only shows entries while the kernel UDP socket is in an active waiting state. DNS queries (UDP:53) have a lifecycle of only a few milliseconds — by the time `netstat` takes a snapshot, the socket has already been released. Linux's `/proc/net/udp` retains socket state information for longer.

**This is expected behavior, not a bug.** The `connection_tracker.rs` implementation correctly parses `netstat` output; the UDP connections are simply too short-lived to be captured.

**Solution (recommendation):** To capture UDP traffic, use `--watch 1` to poll at 1-second intervals, increasing the probability of catching short-lived UDP connections. Future versions could integrate libpcap/BPF for real-time packet capture. No code change is required in the current implementation.

**Takeaway:** Understand the lifetime semantics of the data source before diagnosing missing data as a bug. On macOS, UDP socket visibility in `netstat` is inherently limited by the connectionless nature of the protocol.

---

### Issue 8: ASCII output was unreadable — flat list, no structure, sparse public IP information

**Problem:** The original ASCII output rendered all connections as a flat list, with no distinction between LAN devices and internet connections. Node labels showed bare IPs with no hostnames. Port format (`TCP:443(x3)`) was not concise. There was no summary or top-port information. In real environments with dozens of connections, a single screen of output was impossible to comprehend.

**Root cause:** The initial implementation was a minimum viable version that iterated the edges list directly without semantic grouping or information hierarchy.

**Solution (new format v2, current implementation):**

```
╔══════════════════════════════════════════╗
║           netopo topology                ║
║           2024-01-15 10:30               ║
╚══════════════════════════════════════════╝

━━━ LAN Devices (2) ━━━━━━━━━━━━━━━━━━━━

  [★ mymac.local] (192.168.1.100)
  └─► router.local (192.168.1.1)              TCP:443 x3

━━━ Internet Connections (1) ━━━━━━━━━━━

  Google
  └─► dns.google (8.8.8.8)                   UDP:53

───────────────────────────────────────────────────────────────────────────────
Active: 2  │  TCP: 1  UDP: 1  │  Top ports: 443(x3) 53(x1)
```

Key design decisions:
1. **Hard two-group split**: `is_lan_ip()` classifies IPv4 private ranges and IPv6 local addresses (`fe80::`/`fc`/`fd`/`::1`) as LAN
2. **Internet connections grouped by ISP**: two-layer detection — hostname keywords + IP prefixes (10 major vendors), then MaxMind GeoLite2-ASN DB; unknowns go to "Other"
3. **Hostname-first display**: `node_display()` outputs `"hostname (ip)"` format
4. **Unified port format `PROTO:PORT x N`**, with `--resolve-ports` support for service name translation
5. **Fixed 44-column title box**: `center_in_box()` centers by visual width (CJK = 2 columns), timestamp format `"YYYY-MM-DD HH:MM"`
6. **79-column line width**: `trunc()` is ANSI-aware truncation (skips `\x1b[...m` escape sequences when measuring width)
7. **Max 10 entries per ISP group** by default, with a prompt to use `--all-connections` for more

**Takeaway:** CLI output design is as important as code design. A "good enough" output becomes useless at real data volumes. Define a clear output structure from the first iteration rather than redesigning it after user feedback.
