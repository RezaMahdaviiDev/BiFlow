# ADR 0026: Linux WebKit blank-view workaround

## Status

Accepted

## Context

Packaged AppImage and `.deb` builds opened a window whose WebView painted a
blank surface while JavaScript and IPC still ran (`bootstrap_app` in
`debug.log`) on WebKitGTK 2.52 with a VMware SVGA adapter.

Two separate failures stacked:

1. WebKitGTK 2.44+ defaults to a DMA-BUF renderer that fails to composite on
   VMware, NVIDIA, and some hybrid GPUs. The Tauri-maintainer workaround is
   `WEBKIT_DISABLE_DMABUF_RENDERER=1` before GTK starts, plus
   `WEBKIT_DISABLE_COMPOSITING_MODE=1` on those GPUs
   ([tauri#13183](https://github.com/tauri-apps/tauri/issues/13183),
   [Debian #1078769](https://bugs.debian.org/1078769)). On VMs without a
   working GL path, Mesa also needs `LIBGL_ALWAYS_SOFTWARE=1`
   ([UniClipboard 904009c](https://github.com/UniClipboard/UniClipboard/commit/904009cfd864c4d1be52f0e4a207b238c26d9670)).
2. Closing the window leaves the process in the tray. The next AppImage or
   `.deb` is killed by `tauri-plugin-single-instance` and only the _old_
   window is shown. `debug.log` for 1.2.5 recorded
   `webview.linux_workarounds` in the new process and immediately
   `single_instance_activation` on the still-running 1.2.2 session. The
   plugin's optional `semver` feature only suffixes the major version
   (`1_x_x`), so 1.2.2 and 1.2.5 still share one D-Bus name.

## Decision

Before Tauri starts GTK/WebKit, Linux builds relaunch once with
`WEBKIT_DISABLE_DMABUF_RENDERER=1` unless the user already set it. On virtual
or NVIDIA GPUs they also set `WEBKIT_DISABLE_COMPOSITING_MODE=1`. On virtual
machines they also set `LIBGL_ALWAYS_SOFTWARE=1`. Workspace
`unsafe_code = "forbid"` blocks `std::env::set_var`, so the first process
`exec()`s itself with those variables and `BIFLOW_WEBKIT_WORKAROUNDS=1` to
prevent a loop. `Command::status()` must not wait on a child: that leftover
parent keeps a terminal attached for the whole GUI session. The replacement
process records a structured `webview.linux_workarounds` event after
`debug.log` is opened. Existing environment values are left unchanged.

The single-instance D-Bus name includes the full package version
(`app.biflow.desktop.v1_2_6.SingleInstance`) so launching a newly built
package starts that version's process instead of raising an older tray
instance. Same-version second launches still activate the running window.

## Consequences

AppImage and native Linux windows render the UI on VMware/NVIDIA instead of a
blank view. Intel/AMD keep DMA-BUF disabled (shared-memory fallback) and keep
compositing enabled. Operators can still override the variables before launch.
Two different versions can run at once until the older tray instance is quit;
that is required to test a new package while an older build is still installed.
