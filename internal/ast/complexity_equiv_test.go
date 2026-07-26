package ast

import (
	"math/rand"
	"strings"
	"testing"
)

// legacyComputeCyclomaticComplexity is the previous implementation, kept as the
// oracle for the differential test below.
func legacyComputeCyclomaticComplexity(source string) int {
	cc := 1
	branchKeywords := []string{
		" if ", " else ", " elif ", " elsif ", " elseif ",
		" for ", " while ", " foreach ",
		" case ", " when ",
		" catch ", " except ", " rescue ",
		" && ", " || ",
		"? ",
	}
	lower := " " + strings.ToLower(source) + " "
	for _, kw := range branchKeywords {
		cc += strings.Count(lower, kw)
	}
	return cc
}

func TestComplexityMatchesLegacy(t *testing.T) {
	fixed := []string{
		"",
		" ",
		"if",
		" if ",
		"IF x THEN",
		"else if else if",
		"a && b || c ? d : e",
		"FOR i IN 1..10 LOOP\n IF x THEN\n  CASE WHEN y\n END\n END\nEND LOOP;",
		"func f() { if a && b { for { switch { case 1: } } } }",
		"?  ?  ?",
		"ELSIF ELSEIF ELIF",
		"catch except rescue CATCH EXCEPT RESCUE",
		strings.Repeat(" if  else ", 50),
		"tail keyword at end if",
		"if at start",
	}
	for _, s := range fixed {
		if got, want := ComputeCyclomaticComplexity(s), legacyComputeCyclomaticComplexity(s); got != want {
			t.Errorf("mismatch for %q: got %d, want %d", s, got, want)
		}
	}

	// Randomised differential fuzzing over an alphabet rich in keyword fragments.
	frags := []string{" if ", "IF", " else", "else ", " && ", "||", "? ", "?",
		" for ", "while", " case ", "when", " ", "x", "\n", "\t", "ELSE", "Foreach", "  "}
	rng := rand.New(rand.NewSource(1))
	var b strings.Builder
	for i := 0; i < 3000; i++ {
		b.Reset()
		for j := rng.Intn(30); j > 0; j-- {
			b.WriteString(frags[rng.Intn(len(frags))])
		}
		s := b.String()
		if got, want := ComputeCyclomaticComplexity(s), legacyComputeCyclomaticComplexity(s); got != want {
			t.Fatalf("differential mismatch for %q: got %d, want %d", s, got, want)
		}
	}
}

// TestComplexityBufferReuseIsolation guards the pooled buffer: consecutive calls
// with different lengths must not leak bytes from a previous call.
func TestComplexityBufferReuseIsolation(t *testing.T) {
	long := strings.Repeat(" if ", 200)
	short := "x"
	_ = ComputeCyclomaticComplexity(long)
	if got, want := ComputeCyclomaticComplexity(short), legacyComputeCyclomaticComplexity(short); got != want {
		t.Errorf("short after long: got %d, want %d", got, want)
	}
	if got, want := ComputeCyclomaticComplexity(long), legacyComputeCyclomaticComplexity(long); got != want {
		t.Errorf("long after short: got %d, want %d", got, want)
	}
}
