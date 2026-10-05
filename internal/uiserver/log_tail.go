package uiserver

import (
	"bytes"
	"errors"
	"io"
	"os"
	"strings"
)

// A status poll must not allocate/read a growing daemon log in full.
const maxLogTailBytes = 64 * 1024

func readLogTail(path string, lines int) ([]string, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	info, err := f.Stat()
	if err != nil {
		return nil, err
	}
	start := max(int64(0), info.Size()-maxLogTailBytes)
	data := make([]byte, min(info.Size(), int64(maxLogTailBytes)))
	n, err := f.ReadAt(data, start)
	if err != nil && !errors.Is(err, io.EOF) {
		return nil, err
	}
	data = data[:n]
	if start > 0 {
		// Discard a partial first line, including a UTF-8 codepoint split by the bound.
		// Reading from a single byte before the bound would violate the byte ceiling;
		// dropping this line is safe even if the bound happened to be a line boundary.
		if i := bytes.IndexByte(data, '\n'); i >= 0 {
			data = data[i+1:]
		} else {
			return nil, nil
		}
	}
	tail := splitLastNLocal(string(data), lines)
	for i := range tail {
		tail[i] = strings.TrimSuffix(tail[i], "\r")
	}
	return tail, nil
}
