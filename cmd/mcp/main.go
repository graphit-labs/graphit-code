package main

import (
	"fmt"
	"os"

	"github.com/graphit-labs/graphit-code/internal/daemonctl"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
)

func main() {
	err := mcpproxy.RunProxy(mcpproxy.Config{
		PortFile:     daemonctl.PortFilePath(),
		KeyFile:      daemonctl.KeyFilePath(),
		EnsureDaemon: func() error { _, err := daemonctl.EnsureRunning(); return err },
		Stderr:       os.Stderr,
	}, os.Stdin, os.Stdout)
	if err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
}
