// Command graphit-antlr-sidecar is a standalone binary that parses source code
// using ANTLR grammar drivers. It communicates via a length-prefixed binary
// protocol over stdin/stdout, supporting long-lived mode for amortized startup.
//
// Wire Protocol:
//
//	Request:  [4 bytes length LE uint32][grammar name null-terminated][source bytes]
//	Response: [4 bytes length LE uint32][1 byte status (0=ok, 1=error)][JSON payload]
package main

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"

	antlrcommon "github.com/graphit-labs/graphit-code/internal/ast/antlr/common"
)

// drivers maps grammar names to their ANTLR GrammarDriver implementations.
// Populated by init() functions in driver_*.go files, selected by build tags.
var drivers = map[string]antlrcommon.GrammarDriver{}

const (
	statusOK    = byte(0)
	statusError = byte(1)
)

func main() {
	for {
		grammar, src, err := readRequest(os.Stdin)
		if err != nil {
			if errors.Is(err, io.EOF) || errors.Is(err, io.ErrUnexpectedEOF) {
				os.Exit(0) // Clean shutdown: parent closed stdin.
			}
			writeErrorResponse(os.Stdout, fmt.Sprintf("read request: %v", err))
			os.Exit(1)
		}

		driver, ok := drivers[grammar]
		if !ok {
			writeErrorResponse(os.Stdout, fmt.Sprintf("unknown grammar: %q", grammar))
			continue
		}

		tree, err := driver.Parse(src)
		if err != nil {
			writeErrorResponse(os.Stdout, fmt.Sprintf("parse error (%s): %v", grammar, err))
			continue
		}

		// This process is long-lived and parses request after request, so the
		// grammars' package-level DFA / prediction-context caches would grow
		// unbounded exactly as they do in the indexer. Release them whenever the
		// heap passes the machine-derived budget. Safe here without a barrier:
		// the loop parses one request at a time (and ResetAllCaches locks anyway).
		releaseCachesUnderPressure()

		payload, err := json.Marshal(tree)
		if err != nil {
			writeErrorResponse(os.Stdout, fmt.Sprintf("json marshal: %v", err))
			continue
		}

		writeResponse(os.Stdout, statusOK, payload)
	}
}

// readRequest reads a single request from r.
// Format: [4 bytes length LE uint32][grammar name null-terminated][source bytes]
// The length covers everything after the 4-byte header.
func readRequest(r io.Reader) (grammar string, src []byte, err error) {
	var length uint32
	if err = binary.Read(r, binary.LittleEndian, &length); err != nil {
		return "", nil, err
	}

	buf := make([]byte, length)
	if _, err = io.ReadFull(r, buf); err != nil {
		return "", nil, fmt.Errorf("read payload (%d bytes): %w", length, err)
	}

	// Find null terminator separating grammar name from source.
	idx := bytes.IndexByte(buf, 0)
	if idx < 0 {
		return "", nil, fmt.Errorf("no null terminator in request payload")
	}

	grammar = string(buf[:idx])
	src = buf[idx+1:]
	return grammar, src, nil
}

// writeResponse writes a response frame to w.
// Format: [4 bytes length LE uint32][1 byte status][payload]
func writeResponse(w io.Writer, status byte, payload []byte) {
	length := uint32(1 + len(payload)) // status byte + payload
	_ = binary.Write(w, binary.LittleEndian, length)
	_, _ = w.Write([]byte{status})
	_, _ = w.Write(payload)
}

// writeErrorResponse writes an error response frame.
func writeErrorResponse(w io.Writer, msg string) {
	writeResponse(w, statusError, []byte(msg))
}
