# Task: Add MCP self-relaunch on upgrade

**Date**: 2026-06-19

## Issue
The Graphit daemon automatically detects when a new version is installed by polling `launcher.stamp`. When a change is detected, it spawns a replacement daemon using the new binary and exits.
The MCP proxy binary (`mcp`), however, was not doing this. Because the IDE keeps the MCP process running, the old binary would continue to execute even after an upgrade, while the launcher would point to a new binary.

## Solution
1. Moved the `LauncherStampPath` and `ReadLauncherStamp` helper functions to `internal/daemonctl/daemonctl.go` so they can be safely used by the MCP binary.
2. Implemented `sysutil.ReplaceProcess` which handles transparent process replacement depending on the OS:
   - On Unix (`!windows`), it uses `syscall.Exec` to directly replace the process image, keeping the exact same PID and standard streams.
   - On Windows, it spawns the new binary as a child, closes the parent's `os.Stdin` to abort any pending reads by the current proxy, and waits for the child to exit. The IDE monitors the parent process, which stays alive to proxy the exit state.
3. Added a background `watchLauncherStamp` goroutine to `cmd/mcp/main.go` that polls the launcher stamp every 30 seconds.
4. If the stamp changes, the MCP proxy triggers `sysutil.ReplaceProcess`. Because the PID doesn't change (or the parent stays alive), the IDE continues thinking it is communicating with the original process, and the new binary seamlessly takes over. Since `daemonctl.ResolveExe()` points to the launcher, the arguments `[]string{exe, "mcp", "--stdio"}` are explicitly passed so the launcher knows to route execution back to the MCP component.

5. Refactored the `cmd/launcher` component to use the new `sysutil.ReplaceProcess` and `sysutil.SanitizeInheritedFDs` instead of keeping its own local duplicated implementation, effectively centralizing the cross-platform process replacement logic.

## Files Modified
- `internal/daemonctl/daemonctl.go`
- `cmd/mcp/main.go`
- `internal/sysutil/exec_unix.go` (NEW)
- `internal/sysutil/exec_windows.go` (NEW)
- `cmd/launcher/main.go` (MODIFIED)
- `cmd/launcher/exec_unix.go` (DELETED)
- `cmd/launcher/exec_windows.go` (DELETED)
