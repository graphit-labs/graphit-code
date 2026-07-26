package ast

import (
	"strings"
	"testing"
)

var benchEntitySource = strings.Repeat(`
func handle(x int) error {
	if x > 0 && x < 10 {
		for i := 0; i < x; i++ {
			switch i {
			case 1:
				continue
			}
		}
	} else if x == 0 || x < -5 {
		return nil
	}
	return nil
}
`, 12)

func BenchmarkComputeCyclomaticComplexity(b *testing.B) {
	b.ReportAllocs()
	b.SetBytes(int64(len(benchEntitySource)))
	for i := 0; i < b.N; i++ {
		if ComputeCyclomaticComplexity(benchEntitySource) <= 0 {
			b.Fatal("bad complexity")
		}
	}
}
