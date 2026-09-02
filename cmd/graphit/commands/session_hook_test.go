package commands

import (
	"bytes"
	"errors"
	"os"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/memory"
)

func TestSessionHookCommandRendersFormatPayload(t *testing.T) {
	t.Parallel()

	cmd := newSessionHookCmd()
	cmd.SetArgs([]string{"--format", "additional-context"})
	cmd.SetIn(strings.NewReader("{}"))
	var output bytes.Buffer
	cmd.SetOut(&output)

	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), `"additional_context"`) || !strings.Contains(output.String(), "Graphit session bootstrap") {
		t.Fatalf("unexpected hook output: %s", output.String())
	}
}

func TestLoadMandatoryHookContextReadsBothAuthoritativeScopes(t *testing.T) {
	t.Parallel()

	context, loaded := loadMandatoryHookContextWith(func(scope string) ([]memory.MandatoryEntry, error) {
		return []memory.MandatoryEntry{{Title: scope + " policy", Content: "content for " + scope}}, nil
	})
	if !loaded || !strings.Contains(context, "### project memory: project policy") || !strings.Contains(context, "### user memory: user policy") {
		t.Fatalf("mandatory scopes were not rendered: loaded=%v context=%q", loaded, context)
	}
}

func TestLoadMandatoryHookContextFallsBackWhenAStoreCannotOpen(t *testing.T) {
	t.Parallel()

	_, loaded := loadMandatoryHookContextWith(func(string) ([]memory.MandatoryEntry, error) {
		return nil, errors.New("store unavailable")
	})
	if loaded {
		t.Fatal("store failure must preserve the MCP fallback")
	}
}

func TestSessionHookCommandFirstInvocationDoesNotWaitForCharacterDeviceInput(t *testing.T) {
	input, err := os.Open(os.DevNull)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = input.Close() })

	cmd := newSessionHookCmd()
	cmd.SetArgs([]string{"--format", "first-invocation"})
	cmd.SetIn(input)
	var output bytes.Buffer
	cmd.SetOut(&output)

	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), `"injectSteps"`) || !strings.Contains(output.String(), "Graphit session bootstrap") {
		t.Fatalf("unexpected first-invocation output: %s", output.String())
	}
}

func TestSessionHookCommandFirstInvocationReadsPipedPayload(t *testing.T) {
	t.Parallel()

	cmd := newSessionHookCmd()
	cmd.SetArgs([]string{"--format", "first-invocation"})
	cmd.SetIn(strings.NewReader(`{"invocationNum":1}`))
	var output bytes.Buffer
	cmd.SetOut(&output)

	if err := cmd.Execute(); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(output.String(), "Graphit invariant") || strings.Contains(output.String(), "Graphit session bootstrap") {
		t.Fatalf("non-first invocation did not contain only the invariant: %s", output.String())
	}
}

func TestHookInputNeedsMandatoryOnlyOnFirstInvocation(t *testing.T) {
	t.Parallel()

	if !hookInputNeedsMandatory("first-invocation", []byte(`{"invocationNum":0}`)) {
		t.Fatal("first invocation must load mandatory memory")
	}
	if hookInputNeedsMandatory("first-invocation", []byte(`{"invocationNum":1}`)) {
		t.Fatal("later invocation must not reopen mandatory memory")
	}
	if !hookInputNeedsMandatory("cursor-subagent-task", nil) {
		t.Fatal("Cursor subagent task injection must carry authoritative mandatory memory")
	}
}
