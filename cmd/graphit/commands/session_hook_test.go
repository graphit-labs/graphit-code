package commands

import (
	"bytes"
	"os"
	"strings"
	"testing"
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
	if !strings.Contains(output.String(), `"additional_context"`) || !strings.Contains(output.String(), "graphit_memory_mandatory") {
		t.Fatalf("unexpected hook output: %s", output.String())
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
	if !strings.Contains(output.String(), `"injectSteps"`) || !strings.Contains(output.String(), "graphit_memory_mandatory") {
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
	if strings.TrimSpace(output.String()) != "{}" {
		t.Fatalf("unexpected non-first-invocation output: %s", output.String())
	}
}
