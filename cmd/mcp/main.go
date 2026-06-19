package main

import (
	"fmt"
	"os"
	"time"

	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
	"github.com/graphit-labs/graphit-code/internal/sysutil"
)

// versionCheckInterval is how often the MCP binary polls the launcher stamp.
// The daemon uses 30 s; we use the same value.
const versionCheckInterval = 30 * time.Second

func main() {
	_, _ = daemonctl.EnsureRunning()

	// Record the launcher stamp at startup so we can detect upgrades.
	bootStamp := daemonctl.ReadLauncherStamp()

	if bootStamp != "" {
		go watchLauncherStamp(bootStamp)
	}

	err := mcpproxy.RunProxy(mcpproxy.Config{
		PortFile:     daemonctl.PortFilePath(),
		KeyFile:      daemonctl.KeyFilePath(),
		EnsureDaemon: func() { _, _ = daemonctl.EnsureRunning() },
		Stderr:       os.Stderr,
	}, os.Stdin, os.Stdout)
	if err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
}

func watchLauncherStamp(bootStamp string) {
	ticker := time.NewTicker(versionCheckInterval)
	for range ticker.C {
		current := daemonctl.ReadLauncherStamp()
		if current == "" {
			continue
		}
		if current != bootStamp {
			if exe := daemonctl.ResolveExe(); exe != "" {
				argv := []string{exe, "mcp", "--stdio"}
				_ = sysutil.ReplaceProcess(exe, argv, os.Environ())
			}

			ticker.Stop()
			os.Exit(0)
		}
	}
}
