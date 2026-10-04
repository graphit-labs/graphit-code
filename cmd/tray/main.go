// Command graphit-tray is the system-tray process: daemon status and controls.
//
// It stays resident for the whole login session, so it is built apart from the core
// and links only the tray, its system-tray toolkit and the daemon controls — none of
// the stores, parsers or model runtimes. The launcher runs it for `graphit tray`; the
// `graphit tray login ...` subcommands stay in the core.
package main

import (
	"os"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/tray"
)

func main() {
	if len(os.Args) > 1 {
		output.Fatal("unknown arguments for %s-tray: %v", brand.BinName(), os.Args[1:])
	}
	slogutil.InitFileLogger(brand.GlobalDir())
	err := tray.Run()
	slogutil.CloseFileLogger()
	if err != nil {
		output.Fatal("%s", err)
	}
}
