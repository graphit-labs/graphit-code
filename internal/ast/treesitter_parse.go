package ast

import (
	"context"
	"fmt"
	"os"
	"runtime"
	"sync/atomic"
	"time"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

const defaultNativeParseTimeout = 30 * time.Second

func nativeParseTimeout(opts ParseOptions) time.Duration {
	if opts.NativeTimeout > 0 {
		return opts.NativeTimeout
	}
	if timeout, err := time.ParseDuration(os.Getenv("GRAPHIT_AST_PARSE_TIMEOUT")); err == nil && timeout > 0 {
		return timeout
	}
	return defaultNativeParseTimeout
}

// Tree-sitter retains interrupted parse state. Reset before this parser can
// return to the pool, so another document never resumes the previous one.
func parseNativeTree(p *sitter.Parser, src []byte, opts ParseOptions) (*sitter.Tree, error) {
	if opts.Cancelled != nil && opts.Cancelled() {
		p.Reset()
		return nil, context.Canceled
	}
	timeout := nativeParseTimeout(opts)
	deadline := time.Now().Add(timeout)
	// v0.25.0 leaks the handle saved by ParseWithOptions. Use its native
	// controls until that wrapper releases progress options correctly.
	previousTimeout := p.TimeoutMicros()                       //nolint:staticcheck // Available in the pinned wrapper; avoids its progress handle leak.
	p.SetTimeoutMicros(uint64(max(timeout.Microseconds(), 1))) //nolint:staticcheck // See wrapper limitation above.
	flag := new(uintptr)
	var pin runtime.Pinner
	pin.Pin(flag)
	p.SetCancellationFlag(flag) //nolint:staticcheck // Native pointer is pinned until parsing and the monitor finish.
	defer func() {
		p.Reset()
		p.SetCancellationFlag(nil)          //nolint:staticcheck // Detach before releasing pinned memory or pooling the parser.
		p.SetTimeoutMicros(previousTimeout) //nolint:staticcheck // Restore pooled parser configuration.
		pin.Unpin()
	}()
	stopMonitor := func() {}
	if opts.Cancelled != nil {
		finished, joined := make(chan struct{}), make(chan struct{})
		go func() {
			defer close(joined)
			ticker := time.NewTicker(5 * time.Millisecond)
			defer ticker.Stop()
			for {
				select {
				case <-finished:
					return
				case <-ticker.C:
					if opts.Cancelled() {
						atomic.StoreUintptr(flag, 1)
						return
					}
				}
			}
		}()
		stopMonitor = func() { close(finished); <-joined }
	}
	tree := func() *sitter.Tree {
		defer stopMonitor()
		return p.Parse(src, nil)
	}()
	if atomic.LoadUintptr(flag) != 0 || (opts.Cancelled != nil && opts.Cancelled()) {
		if tree != nil {
			tree.Close()
		}
		return nil, context.Canceled
	}
	if !time.Now().Before(deadline) {
		if tree != nil {
			tree.Close()
		}
		return nil, fmt.Errorf("tree-sitter native parse exceeded %s: %w", timeout, context.DeadlineExceeded)
	}
	if tree == nil {
		return nil, fmt.Errorf("tree-sitter parse returned nil tree")
	}
	return tree, nil
}
