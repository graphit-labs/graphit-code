package shared

import (
	"encoding/binary"
	"io"
	"os"
	"strings"

	"github.com/antlr4-go/antlr/v4"
)

// ParseRequest bundles the source code with parser metadata so the
// driver can both parse and serialize in one call.
type ParseResult struct {
	Tree antlr.ParseTree
	Meta ParserMeta
}

// ParseFunc receives raw source and returns a parse result.
// It is called once per IPC request.
type ParseFunc func(source string) ParseResult

// Run is the main entry point for all ANTLR grammar binaries.
//
// It redirects os.Stdout to /dev/null (protecting the binary IPC wire
// from any stray fmt.Println calls inside the ANTLR runtime), then
// enters a persistent loop reading length-prefixed requests from stdin
// and writing length-prefixed JSON responses to the saved stdout fd.
//
// Protocol:
//
//	Request:  [4 bytes BE uint32 length][source bytes]
//	Response: [4 bytes BE uint32 length][JSON parse tree]
func Run(parse ParseFunc) {
	stdin := os.Stdin

	// Save the real stdout for IPC output, then redirect os.Stdout to
	// /dev/null so any stray ANTLR fmt.Println calls are harmlessly absorbed.
	// This is necessary because ANTLR's DefaultErrorStrategy.ReportError()
	// unconditionally calls fmt.Println("unknown recognition error type: ...")
	// for unhandled error types, which corrupts the binary IPC wire.
	ipcOut := os.Stdout
	devNull, err := os.Open(os.DevNull)
	if err == nil {
		os.Stdout = devNull
		defer devNull.Close()
	}

	var lenBuf [4]byte

	for {
		if _, err := io.ReadFull(stdin, lenBuf[:]); err != nil {
			break // stdin closed — clean exit
		}

		srcLen := binary.BigEndian.Uint32(lenBuf[:])
		source := make([]byte, srcLen)
		if _, err := io.ReadFull(stdin, source); err != nil {
			os.Exit(1)
		}

		result := parse(string(source))

		var json string
		if result.Tree == nil {
			json = `{"type":"error","message":"parse_error"}`
		} else {
			var out strings.Builder
			out.Grow(len(source) * 2)
			TreeToJSON(&out, result.Tree, result.Meta)
			json = out.String()
		}

		binary.BigEndian.PutUint32(lenBuf[:], uint32(len(json)))
		ipcOut.Write(lenBuf[:])
		ipcOut.Write([]byte(json))
	}
}
