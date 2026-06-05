package main

import (
	"fmt"
	"io"
	"net"
	"os"
	"path/filepath"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func main() {
	ensureDaemonRunning()

	sockFile := filepath.Join(brand.GlobalDir(), "daemon", "mcp.sock")

	var conn net.Conn
	var err error
	for i := 0; i < 20; i++ {
		conn, err = net.Dial("unix", sockFile)
		if err == nil {
			break
		}
		time.Sleep(250 * time.Millisecond)
	}
	if err != nil {
		fmt.Fprintf(os.Stderr, "failed to connect to daemon mcp.sock: %v\n", err)
		os.Exit(1)
	}
	defer conn.Close()

	errc := make(chan error, 2)
	go func() {
		_, e := io.Copy(conn, os.Stdin)
		errc <- e
	}()
	go func() {
		_, e := io.Copy(os.Stdout, conn)
		errc <- e
	}()
	<-errc
}
