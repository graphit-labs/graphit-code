package ast

import (
	"fmt"
	"math/rand"
	"strings"
	"testing"
)

func legacyResolveReceiverTypes(result *ParsedFile, src []byte, lang string, langConfig *ExternalQueryFile) {
	if len(result.CallSites) == 0 {
		return
	}
	lines := strings.Split(string(src), "\n")
	methodToClass := make(map[string]string)
	for _, dataKey := range []string{"functions", "methods"} {
		for _, e := range result.Entities[dataKey] {
			if e.Context != "" && (e.ContextType == "Class" || e.ContextType == "Struct") {
				methodToClass[e.Name] = e.Context
			}
		}
	}
	selfKeywords := selfKeywordsForLang(lang, langConfig)
	for i := range result.CallSites {
		call := &result.CallSites[i]
		if strings.HasPrefix(call.FullName, "new:") {
			call.ReceiverType = strings.TrimPrefix(call.FullName, "new:")
			continue
		}
		if call.SourceName != "" && len(selfKeywords) > 0 {
			className := methodToClass[call.SourceName]
			if className == "" {
				continue
			}
			lineIdx := call.Line - 1
			if lineIdx < 0 || lineIdx >= len(lines) {
				continue
			}
			lineText := lines[lineIdx]
			for _, kw := range selfKeywords {
				if strings.Contains(lineText, kw+call.Name) {
					call.ReceiverType = className
					break
				}
			}
		}
	}
}

func makeReceiverFixture(rng *rand.Rand) (*ParsedFile, *ParsedFile, []byte) {
	var b strings.Builder
	nLines := rng.Intn(40)
	for i := 0; i < nLines; i++ {
		switch rng.Intn(4) {
		case 0:
			b.WriteString("  this.doThing()\n")
		case 1:
			b.WriteString("  self.doThing()\n")
		case 2:
			b.WriteString("  plain line\n")
		default:
			b.WriteString("\n")
		}
	}
	if rng.Intn(2) == 0 && b.Len() > 0 {
		s := b.String()
		b.Reset()
		b.WriteString(strings.TrimSuffix(s, "\n"))
	}
	src := []byte(b.String())

	mk := func() *ParsedFile {
		pf := &ParsedFile{Entities: map[string][]Entity{
			"methods": {{Name: "doThing", Context: "Widget", ContextType: "Class"}},
		}}
		for i := 0; i < rng.Intn(6); i++ {
			pf.CallSites = append(pf.CallSites, CallInfo{
				Name:       "doThing",
				SourceName: []string{"doThing", "", "other"}[rng.Intn(3)],
				FullName:   []string{"", "new:Widget", "x"}[rng.Intn(3)],
				Line:       rng.Intn(nLines+4) - 1,
			})
		}
		return pf
	}
	a := mk()
	bcopy := &ParsedFile{Entities: a.Entities, CallSites: append([]CallInfo(nil), a.CallSites...)}
	return a, bcopy, src
}

func TestResolveReceiverTypesMatchesLegacy(t *testing.T) {
	cfg := &ExternalQueryFile{SelfKeywords: []string{"this.", "self."}}
	rng := rand.New(rand.NewSource(7))
	for iter := 0; iter < 2000; iter++ {
		newPF, oldPF, src := makeReceiverFixture(rng)
		resolveReceiverTypes(newPF, src, "go", cfg)
		legacyResolveReceiverTypes(oldPF, src, "go", cfg)
		for i := range newPF.CallSites {
			if newPF.CallSites[i].ReceiverType != oldPF.CallSites[i].ReceiverType {
				t.Fatalf("iter %d call %d: got %q want %q (line=%d src=%q)",
					iter, i, newPF.CallSites[i].ReceiverType, oldPF.CallSites[i].ReceiverType,
					newPF.CallSites[i].Line, string(src))
			}
		}
	}
}

func benchReceiverInput() (*ParsedFile, []byte, *ExternalQueryFile) {
	var b strings.Builder
	for i := 0; i < 4000; i++ {
		fmt.Fprintf(&b, "  this.doThing() // line %d\n", i)
	}
	src := []byte(b.String())
	pf := &ParsedFile{
		Entities:  map[string][]Entity{"methods": {{Name: "doThing", Context: "Widget", ContextType: "Class"}}},
		CallSites: []CallInfo{{Name: "doThing", SourceName: "doThing", Line: 3999}},
	}
	return pf, src, &ExternalQueryFile{SelfKeywords: []string{"this."}}
}

func BenchmarkResolveReceiverTypes(b *testing.B) {
	_, src, cfg := benchReceiverInput()
	b.ReportAllocs()
	b.SetBytes(int64(len(src)))
	for i := 0; i < b.N; i++ {
		pf := &ParsedFile{
			Entities:  map[string][]Entity{"methods": {{Name: "doThing", Context: "Widget", ContextType: "Class"}}},
			CallSites: []CallInfo{{Name: "doThing", SourceName: "doThing", Line: 3999}},
		}
		resolveReceiverTypes(pf, src, "go", cfg)
	}
}

func BenchmarkResolveReceiverTypesLegacy(b *testing.B) {
	_, src, cfg := benchReceiverInput()
	b.ReportAllocs()
	b.SetBytes(int64(len(src)))
	for i := 0; i < b.N; i++ {
		pf := &ParsedFile{
			Entities:  map[string][]Entity{"methods": {{Name: "doThing", Context: "Widget", ContextType: "Class"}}},
			CallSites: []CallInfo{{Name: "doThing", SourceName: "doThing", Line: 3999}},
		}
		legacyResolveReceiverTypes(pf, src, "go", cfg)
	}
}
