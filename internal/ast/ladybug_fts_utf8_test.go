package ast

import (
	"fmt"
	"path/filepath"
	"testing"
	"unicode/utf8"

	lbug "github.com/LadybugDB/go-ladybug"
)

// TestLadybugFTSRejectsControlCharacters finds what makes CREATE_FTS_INDEX fail on the real
// corpus with
//
//	Runtime exception: Failed calling LOWER: Invalid UTF-8.
//
// The corpus is not the problem: all 35358 files decode as valid UTF-8, verified byte by
// byte. What they do contain, from CP1252 text converted as Latin-1, are C1 control
// characters — U+0083 appears 915 times and U+0087 761 times — which are legal UTF-8 but
// unusual, alongside 84 ordinary accented codepoints.
//
// This narrows "invalid UTF-8" to a specific class of input, which decides whether the fix
// belongs upstream, in our sanitisation, or both.
func TestLadybugFTSRejectsControlCharacters(t *testing.T) {
	dir := t.TempDir()
	db, err := lbug.OpenDatabase(filepath.Join(dir, "utf8"), lbug.DefaultSystemConfig())
	if err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer db.Close()
	conn, err := lbug.OpenConnection(db)
	if err != nil {
		t.Fatalf("conn: %v", err)
	}
	defer conn.Close()

	run := func(q string) error {
		r, e := conn.Query(q)
		if e != nil {
			return e
		}
		r.Close()
		return nil
	}
	_ = run("INSTALL fts")
	if err := run("LOAD EXTENSION fts"); err != nil {
		t.Skipf("fts unavailable: %v", err)
	}

	// Multi-byte text repeated to a target byte size. If LOWER processes in fixed-size
	// chunks it will eventually split a two-byte character, and the size at which that
	// starts failing is the answer.
	sized := func(target int) string {
		unit := "criação de índice não padrão para pedido \u0083 x "
		out := make([]byte, 0, target+len(unit))
		for len(out) < target {
			out = append(out, unit...)
		}
		return string(out)
	}

	cases := []struct {
		name string
		text string
	}{
		{"plain ascii", "create table pedido"},
		{"accented latin-1 range", "criação de índice não padrão"},
		{"inverted question mark", "coluna ¿ desconhecida"},
		{"C1 control U+0083", "valor \u0083 suspeito"},
		{"C1 control U+0087", "valor \u0087 suspeito"},
		{"C0 control U+0001", "valor \u0001 suspeito"},
		{"null byte U+0000", "valor \u0000 suspeito"},
		{"multibyte 4 KiB", sized(4 << 10)},
		{"multibyte 64 KiB", sized(64 << 10)},
		{"multibyte 256 KiB", sized(256 << 10)},
		{"multibyte 704 KiB (largest file)", sized(704 << 10)},
		{"multibyte 2 MiB", sized(2 << 20)},
	}

	t.Logf("%-34s | %-8s | %s", "input", "valid?", "CREATE_FTS_INDEX")
	t.Logf("%s", "----------------------------------------------------------------")

	var rejected []string
	for i, c := range cases {
		table := fmt.Sprintf("U%d", i)
		if err := run(fmt.Sprintf(
			"CREATE NODE TABLE %s(uid STRING, body STRING, PRIMARY KEY(uid))", table)); err != nil {
			t.Fatalf("schema %s: %v", table, err)
		}

		stmt, err := conn.Prepare(fmt.Sprintf("CREATE (:%s {uid: $uid, body: $body})", table))
		if err != nil {
			t.Fatalf("prepare %s: %v", table, err)
		}
		res, err := conn.Execute(stmt, map[string]any{"uid": "r1", "body": c.text})
		stmt.Close()
		if err != nil {
			t.Logf("%-34s | %-8v | insert rejected: %v", c.name, utf8.ValidString(c.text), err)
			rejected = append(rejected, c.name+" (insert)")
			continue
		}
		res.Close()

		err = run(fmt.Sprintf("CALL CREATE_FTS_INDEX('%s', 'idx%d', ['body'])", table, i))
		status := "accepted"
		if err != nil {
			status = "REJECTED: " + err.Error()
			rejected = append(rejected, c.name)
		}
		t.Logf("%-34s | %-8v | %s", c.name, utf8.ValidString(c.text), status)
	}

	if len(rejected) == 0 {
		t.Log("no input class was rejected — the corpus failure has another cause")
		return
	}
	t.Logf("rejected: %v", rejected)

	// Every input here is valid UTF-8 by Go's definition, so a rejection means the engine's
	// notion of valid is narrower than the standard's. That is worth reporting upstream and
	// worth sanitising on our side, since the corpus cannot be changed.
	for _, c := range cases {
		if !utf8.ValidString(c.text) {
			t.Fatalf("test bug: %q is not valid UTF-8, so a rejection would prove nothing", c.name)
		}
	}
}
