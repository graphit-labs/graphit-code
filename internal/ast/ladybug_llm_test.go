package ast

import (
	"fmt"
	"path/filepath"
	"strings"
	"testing"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugLLMExtension establishes what LadybugDB's LLM extension actually
// provides, because it decides whether consolidating search onto Ladybug could
// also absorb the embedding step and retire the ONNX/CodeRankEmbed pipeline.
//
// liblbug 0.18.2 advertises CREATE_EMBEDDING and "Adds support for LLM
// operations" in its extension catalogue, but the catalogue lives in the core
// library while extensions ship as separate downloadable objects — so the strings
// prove the function exists, not what backs it. The question that matters is
// whether it runs a model locally or calls out to a hosted provider: the project
// embeds ~1M entities during a full index, and a per-call network dependency
// would be a different system entirely.
//
// This probe records the answer rather than asserting a preference; the error text
// from an under-specified call is what names the required provider arguments.
func TestLadybugLLMExtension(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "llm"), lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer db.Close()
	conn, err := lbug.OpenConnection(db)
	if err != nil {
		t.Fatalf("conn: %v", err)
	}
	defer conn.Close()

	query := func(q string) (string, error) {
		r, e := conn.Query(q)
		if e != nil {
			return "", e
		}
		defer r.Close()
		var rows []string
		for r.HasNext() {
			tup, err := r.Next()
			if err != nil {
				break
			}
			var cells []string
			for i := uint64(0); ; i++ {
				v, err := tup.GetValue(i)
				if err != nil {
					break
				}
				s := strings.TrimSpace(strings.ReplaceAll(fmt.Sprint(v), "\n", " "))
				if len(s) > 120 {
					s = s[:120] + "…"
				}
				cells = append(cells, s)
				if i > 8 {
					break
				}
			}
			rows = append(rows, strings.Join(cells, " | "))
		}
		return strings.Join(rows, "\n"), nil
	}

	if _, err := query("INSTALL llm"); err != nil {
		t.Skipf("INSTALL llm failed (offline or extension unavailable): %v", err)
	}
	if _, err := query("LOAD EXTENSION llm"); err != nil {
		t.Skipf("llm extension cannot load: %v", err)
	}
	t.Log("llm extension loaded")

	// CREATE_EMBEDDING is a scalar function, not a table function ("CALL" is
	// rejected by the binder), so it is invoked in a RETURN expression.
	//
	// Under-specified first, on purpose: the error enumerates what the function
	// demands, which is what distinguishes a locally executed model from a call to
	// a hosted provider.
	probes := []string{
		"RETURN CREATE_EMBEDDING('hello world')",
		"RETURN CREATE_EMBEDDING('hello world', 'open-ai', 'text-embedding-3-small')",
		"RETURN CREATE_EMBEDDING('hello world', 'ollama', 'nomic-embed-text')",
		"RETURN CREATE_EMBEDDING('hello world', 'local', 'default')",
	}
	anyAccepted := false
	for _, q := range probes {
		out, err := query(q)
		if err != nil {
			t.Logf("%s\n  -> %v", q, err)
			continue
		}
		anyAccepted = true
		t.Logf("%s\n  -> ACCEPTED: %s", q, out)
	}

	if !anyAccepted {
		t.Log("no CREATE_EMBEDDING signature was accepted without further configuration — " +
			"see the argument errors above for what the extension requires")
	}
}
