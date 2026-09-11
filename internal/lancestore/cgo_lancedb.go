//go:build lancedb

package lancestore

/*
#cgo LDFLAGS: -L${SRCDIR}/../../.native -llancedb_go
#cgo LDFLAGS: -Wl,-rpath,${SRCDIR}/../../.native
#cgo LDFLAGS: -Wl,-rpath,$ORIGIN
*/
import "C"
