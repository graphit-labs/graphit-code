package ast

import (
	"sync"
	"testing"
)

// TestResetAntlrCachesRace reproduces the scenario that a per-pipeline barrier
// does NOT cover: one goroutine resetting the grammars' package-level
// ATN/DFA/PredictionContextCache while others construct parsers and parse. That
// happens for real in the daemon, which runs a pipeline per project (and serves
// MCP queries) inside one process.
//
// Run with -race; without the shared read/write lock this reports a data race
// between caches.go's field assignment and NewXxxParser reading decisionToDFA.
func TestResetAntlrCachesRace(t *testing.T) {
	const (
		parsers = 8
		rounds  = 25
	)
	src := []byte("CREATE OR REPLACE FUNCTION f RETURN NUMBER IS BEGIN RETURN 1; END;\n")

	drv := nativeAntlrDrivers["antlr-plsql"]
	if drv == nil {
		t.Fatal("antlr-plsql driver not registered")
	}
	// Warm the ATN once so the sync.Once init is not the thing under test.
	_, _ = drv.Parse(src)

	var wg sync.WaitGroup
	stop := make(chan struct{})

	for i := 0; i < parsers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for {
				select {
				case <-stop:
					return
				default:
					_, _ = drv.Parse(src)
				}
			}
		}()
	}

	for i := 0; i < rounds; i++ {
		ResetAntlrCaches()
	}
	close(stop)
	wg.Wait()
}
