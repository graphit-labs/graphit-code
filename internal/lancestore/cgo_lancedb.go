//go:build lancedb

package lancestore

// The link contract for the LanceDB native, declared HERE and not left to the environment.
//
// `lancedb-go` supplies its own header (`#cgo CFLAGS: -I${SRCDIR}/../../include`) and NO
// LDFLAGS at all, so the library has to come from somewhere. Leaving that to CGO_LDFLAGS means
// every `go build` and every `go test` needs two exported variables to work, which is the same
// class of trap as the fts5 tag: it does not fail at the flag, it fails later and elsewhere.
//
// ${SRCDIR} is expanded by cgo to the absolute directory of THIS file, so both paths below are
// absolute without anything being hardcoded, and a plain `go test -tags lancedb ./...` links.
//
// TWO rpaths, because there are two kinds of binary:
//   - ${SRCDIR}/... — an absolute path, for test binaries, which the toolchain links in a
//     temporary directory where nothing sits beside them;
//   - $ORIGIN — for the shipped binary, which travels with the library next to it. The loader
//     expands it at run time, and it is what makes the install relocatable.

/*
#cgo LDFLAGS: -L${SRCDIR}/../../.native -llancedb_go
#cgo LDFLAGS: -Wl,-rpath,${SRCDIR}/../../.native
#cgo LDFLAGS: -Wl,-rpath,$ORIGIN
*/
import "C"
