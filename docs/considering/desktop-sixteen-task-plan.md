# Implementation plan: updater, rules, responsive shell, tray

Assessment of the current tree (2026-08-15) and the work for the sixteen
requested tasks. Runtime changes land in numbered commits on this branch.

## Current state

| Area                | Today                                                                                                                                                                               |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| App updater         | Signed Tauri plugin, AppImage/NSIS self-replace, `.deb` opens Releases. Check retries 4×. Install-time `check()` has no retry, no mutex, no timeout.                                |
| Rules / third-party | Cloud sync is manual, SHA-validated, last-good fallback. No HTTP retry, per-file persist (not a staging generation). Mihomo/Hiddify are install-once, not part of `install_update`. |
| Shell               | Fixed 1120×760, sidebar only, no bottom nav, Dashboard `overflow-hidden`, status bar Advanced-only.                                                                                 |
| About Basic         | `UiModeSwitch` works, but Basic + `page === "about"` keeps About instead of BasicDashboard.                                                                                         |
| IP / traffic        | IP is display-only. No byte counters.                                                                                                                                               |
| Connect             | Does not auto-install helper/Hiddify/Mihomo.                                                                                                                                        |
| First launch        | Default **Advanced** (ADR 0023).                                                                                                                                                    |
| Glow                | 2px, `border-radius: 0.5rem`.                                                                                                                                                       |
| Window              | Not resizable; size not persisted.                                                                                                                                                  |
| Context menu        | Global `preventDefault` on every `contextmenu`.                                                                                                                                     |
| Tray                | Eight static items; Connect/Pause/Resume/Disconnect all visible.                                                                                                                    |

## Per-task approach

1. **Complete update flow** — Extend `check_for_update` / `install_update` so a single operator action covers the signed app package plus versioned Iran rules and bundled Mihomo. Keep Tauri updater for the binary.
2. **Reliable update check** — Per-attempt timeout, try-lock concurrency, install-time retry, download timeout. Background poll stays silent on failure.
3. **Reliable rule sync** — Fetch retry + backoff + direct fallback, staging directory, publish files then meta, last-good cache on any failure.
4. **Responsive UI** — CSS/layout breakpoints. Viewport `<768`: bottom nav (not hamburger). Status bar and bottom nav are stacked siblings, never overlaid.
5. **About Basic** — Switching to Basic sets `page` to `dashboard` so the same control matches other pages.
6. **Dashboard scroll** — `overflow-y-auto` on the Dashboard section.
7. **Sticky status bar** — Always rendered (Basic and Advanced), flex sibling after scroll content, `shrink-0`.
8. **Clickable IP** — Button refreshes network status; in-flight guard; checking spinner.
9. **Traffic totals** — Mihomo `/connections` totals folded into a persisted lifetime file so disconnect/reconnect does not zero the bar.
10. **Connect installs deps** — Before `start()`, sequentially install missing helper → Hiddify → Mihomo; abort with the step error.
11. **First-launch Basic** — `readUiMode()` defaults to `basic`; Connect-install from (10) runs in Basic.
12. **Glow** — `border-radius: 0`, border 3px, stronger inset/outer glow. Overlay still, not a layout-shifting shell border.
13. **Window size** — Resizable; min 390×640; default 1120×760; persist logical size; clamp to work area.
14. **Three viewports** — Playwright + browser screenshots at 390×844, 768×1024, 1024×768; fix clipping.
15. **Input context menu** — Allow `contextmenu` on text/number inputs; Select All / Copy / Cut / Paste; disable when inapplicable.
16. **Tray** — Exactly Connect\|Disconnect, Pause\|Resume, Quit, with separators; rebuild on every snapshot.
17. **Connection lock** — One shared lifecycle lock (`connecting` / `disconnecting` / `pausing` / `resuming`) in the engine, tray, and UI. Reject conflicting operations; restore controls after success, failure, or timeout.
18. **Button icons** — Lucide icons on every button; icon-only controls keep a tooltip and accessible name.
19. **In-button progress** — Done: `operation_stage` on the snapshot, standalone card removed, fill and stage label live on the active connection button.
20. **Connect glow** — Subtle available-state glow on Connect; off while disabled/processing; honor `prefers-reduced-motion`.

## Platform notes

- Linux AppImage and Windows NSIS self-replace; `.deb` stays manual (ADR 0024).
- Native Windows UI is verified with `cargo xwin clippy` / contract tests in this environment, not a Windows desktop session.
- Helper install still needs an interactive polkit/UAC prompt; Connect-install surfaces that error instead of hanging.
