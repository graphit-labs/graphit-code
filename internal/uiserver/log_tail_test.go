package uiserver

import (
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
	"unicode/utf8"
)

func TestUILogTailBoundedAndCompleteLines(t *testing.T) {
	cases := []struct {
		name, data string
		n          int
		want       []string
	}{
		{"empty", "", 50, []string{}},
		{"no-final-newline", "um\ndois", 50, []string{"um", "dois"}},
		{"CRLF", "um\r\ndois\r\n", 50, []string{"um", "dois"}},
		{"UTF8", "ação\nmemória\n", 1, []string{"memória"}},
		{"huge-line", strings.Repeat("x", maxLogTailBytes+10), 50, nil},
		{"truncated-first-line", strings.Repeat("á", maxLogTailBytes) + "\nfim", 50, []string{"fim"}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			path := filepath.Join(t.TempDir(), "daemon.log")
			if err := os.WriteFile(path, []byte(tc.data), 0o600); err != nil {
				t.Fatal(err)
			}
			got, err := readLogTail(path, tc.n)
			if err != nil || !reflect.DeepEqual(got, tc.want) {
				t.Fatalf("got=%v want=%v err=%v", got, tc.want, err)
			}
			for _, line := range got {
				if !utf8.ValidString(line) {
					t.Fatal("returned a split UTF-8 codepoint")
				}
			}
		})
	}
	path := filepath.Join(t.TempDir(), "daemon.log")
	var b strings.Builder
	for i := 0; i < 100000; i++ {
		fmt.Fprintf(&b, "line %d\n", i)
	}
	if err := os.WriteFile(path, []byte(b.String()), 0o600); err != nil {
		t.Fatal(err)
	}
	got, err := readLogTail(path, 50)
	if err != nil || len(got) != 50 || got[0] != "line 99950" || got[49] != "line 99999" {
		t.Fatalf("large log tail: %v %v", got, err)
	}
	if _, err := readLogTail(path+"-missing", 50); !os.IsNotExist(err) {
		t.Fatal("missing log should report read error")
	}
}

func BenchmarkUILogTailLargeFile(b *testing.B) {
	path := filepath.Join(b.TempDir(), "daemon.log")
	if err := os.WriteFile(path, []byte(strings.Repeat("a representative log entry\n", 400000)), 0o600); err != nil {
		b.Fatal(err)
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if _, err := readLogTail(path, 50); err != nil {
			b.Fatal(err)
		}
	}
}
