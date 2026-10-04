package ast

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	sitter "github.com/tree-sitter/go-tree-sitter"
)

func TestTreeSitterNativeInterruptedAndReuse(t *testing.T) {
	for _, reason := range []string{"cancel", "timeout", "pre-cancel"} {
		t.Run(reason, func(t *testing.T) {
			p := sitter.NewParser()
			defer p.Close()
			if err := p.SetLanguage(NativeLanguage("go")); err != nil {
				t.Fatal(err)
			}
			const previousTimeout = 123456
			p.SetTimeoutMicros(previousTimeout) //nolint:staticcheck // Verify preservation of pooled native settings.
			src := []byte("package demo\n" + strings.Repeat("func Work() { println(1) }\n", 100000))
			checks := 0
			opts := ParseOptions{NativeTimeout: time.Second}
			want := context.Canceled
			if reason == "cancel" {
				opts.Cancelled = func() bool { checks++; return checks >= 3 }
			} else if reason == "timeout" {
				opts.NativeTimeout = time.Millisecond
				want = context.DeadlineExceeded
			} else {
				opts.Cancelled = func() bool { return true }
			}
			started := time.Now()
			tree, err := parseNativeTree(p, src, opts)
			if tree != nil || !errors.Is(err, want) {
				t.Fatalf("interruption: tree=%v err=%v, want %v", tree, err, want)
			}
			if time.Since(started) > 2*time.Second {
				t.Fatal("native parsing did not stop promptly")
			}
			if reason == "cancel" && checks < 3 {
				t.Fatal("cancellation was not observed during native parsing")
			}
			if p.CancellationFlag() != nil || p.TimeoutMicros() != previousTimeout { //nolint:staticcheck // Verify cleanup of the pinned wrapper's native controls.
				t.Fatal("interrupted parser retained cancellation or timeout state")
			}
			clean := []byte("package next\nfunc Fresh() {}\n")
			tree, err = parseNativeTree(p, clean, ParseOptions{NativeTimeout: time.Second})
			if err != nil {
				t.Fatalf("reuse: %v", err)
			}
			defer tree.Close()
			root := tree.RootNode()
			if root.HasError() || root.EndByte() != uint(len(clean)) || root.NamedChildCount() != 2 {
				t.Fatal("parser resumed interrupted document instead of parsing fresh input")
			}
			if p.CancellationFlag() != nil || p.TimeoutMicros() != previousTimeout { //nolint:staticcheck // Success must also release the native controls.
				t.Fatal("successful parser retained cancellation or timeout state")
			}
		})
	}
}

func TestTreeSitterNativeTimeoutConfiguration(t *testing.T) {
	for _, value := range []string{"", "invalid", "0s", "-1s"} {
		t.Setenv("GRAPHIT_AST_PARSE_TIMEOUT", value)
		if got := nativeParseTimeout(ParseOptions{}); got != defaultNativeParseTimeout {
			t.Fatalf("%q selected %s", value, got)
		}
	}
	t.Setenv("GRAPHIT_AST_PARSE_TIMEOUT", "2m")
	if got := nativeParseTimeout(ParseOptions{}); got != 2*time.Minute {
		t.Fatal(got)
	}
	if got := nativeParseTimeout(ParseOptions{NativeTimeout: time.Second}); got != time.Second {
		t.Fatal(got)
	}
}

func TestTreeSitterNativeCompositeCancellation(t *testing.T) {
	_, err := NewCompositeParser("", nil).Parse("cancelled.unknown", false, ParseOptions{Cancelled: func() bool { return true }})
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("cancelled operation attempted grammar resolution: %v", err)
	}
}
