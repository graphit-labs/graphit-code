package main

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/mcpproxy"
)

func main() {
	ensureDaemonRunning()

	sockFile := filepath.Join(brand.GlobalDir(), "daemon", "mcp.sock")

	err := mcpproxy.RunProxy(mcpproxy.Config{
		SockFile:     sockFile,
		EnsureDaemon: ensureDaemonRunning,
		Stderr:       os.Stderr,
	}, os.Stdin, os.Stdout)
	if err != nil {
		fmt.Fprintf(os.Stderr, "%v\n", err)
		os.Exit(1)
	}
}
