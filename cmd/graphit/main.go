package main

import (
	"os"
	"os/signal"
	"syscall"

	"github.com/graphit-labs/graphit-code/cmd/graphit/commands"
	"github.com/graphit-labs/graphit-code/internal/output"
)

func main() {

	c := make(chan os.Signal, 1)
	signal.Notify(c, os.Interrupt, syscall.SIGTERM)
	go func() {
		<-c
		output.Interrupted()
	}()

	commands.Execute()
}
