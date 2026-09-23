package main

import (
	"fmt"
	"os"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
	"github.com/graphit-labs/graphit-code/internal/tray"
)

func main() {
	var trayOnce sync.Once
	err := mcpproxy.RunProxy(mcpproxy.Config{
		PortFile: daemonctl.PortFilePath(),
		KeyFile:  daemonctl.KeyFilePath(),
		EnsureDaemon: func() error {
			if _, err := daemonctl.EnsureRunning(); err != nil {
				return err
			}
			trayOnce.Do(func() {
				if err := tray.EnsureRunning(); err != nil {
					fmt.Fprintf(os.Stderr, "[mcp-proxy] starting tray failed: %v\n", err)
				}
			})
			return nil
		},
		Stderr: os.Stderr,
	}, os.Stdin, os.Stdout)
	if err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
}
