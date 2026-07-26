package ast

import (
	"bytes"
	"os"
	"sync"
)

func ReadFileBytes(path string) ([]byte, error) {
	return os.ReadFile(path)
}

// branchKeywordsBytes mirrors the branch keywords as byte slices, built once so
// the hot path can use bytes.Count without allocating a []byte per call.
var branchKeywordsBytes = func() [][]byte {
	kws := []string{
		" if ", " else ", " elif ", " elsif ", " elseif ",
		" for ", " while ", " foreach ",
		" case ", " when ",
		" catch ", " except ", " rescue ",
		" && ", " || ",
		"? ",
	}
	out := make([][]byte, len(kws))
	for i, k := range kws {
		out[i] = []byte(k)
	}
	return out
}()

// lowerBufPool reuses the lowercased scratch buffer across calls. Complexity is
// computed once per extracted entity, so the previous strings.ToLower copy cost
// one allocation the size of every entity body in the repository.
var lowerBufPool = sync.Pool{
	New: func() any {
		b := make([]byte, 0, 4096)
		return &b
	},
}

// ComputeCyclomaticComplexity counts branch-introducing keywords in an entity's
// source. It builds " " + lower(source) + " " in a pooled buffer and counts each
// keyword with bytes.Count, which is byte-for-byte equivalent to the previous
// strings.ToLower + strings.Count implementation (same virtual haystack, same
// per-keyword non-overlapping counting) while allocating nothing per call.
func ComputeCyclomaticComplexity(source string) int {
	cc := 1

	bufp := lowerBufPool.Get().(*[]byte)
	buf := (*bufp)[:0]
	if cap(buf) < len(source)+2 {
		buf = make([]byte, 0, len(source)+2)
	}
	buf = append(buf, ' ')
	for i := 0; i < len(source); i++ {
		c := source[i]
		if c >= 'A' && c <= 'Z' {
			c += 'a' - 'A'
		}
		buf = append(buf, c)
	}
	buf = append(buf, ' ')

	for _, kw := range branchKeywordsBytes {
		cc += bytes.Count(buf, kw)
	}

	*bufp = buf
	lowerBufPool.Put(bufp)
	return cc
}
