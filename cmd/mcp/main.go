package main

import (
	"fmt"
	"os"

	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
)

func main() {
	_, _ = daemonctl.EnsureRunning()

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
