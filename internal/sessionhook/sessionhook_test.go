package sessionhook

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestProtocolPreservesTheOrderedTwoPhaseRecall(t *testing.T) {
	t.Parallel()

	protocol := Protocol()
	mandatory := strings.Index(protocol, "graphit_memory_mandatory")
	contextual := strings.Index(protocol, "graphit_memory_search")
	if mandatory < 0 || contextual < 0 || mandatory >= contextual {
		t.Fatalf("protocol does not order mandatory recall before contextual search:\n%s", protocol)
	}
	if !strings.Contains(protocol, "exclude_mandatory: true") {
		t.Fatalf("protocol does not exclude mandatory memories from contextual search:\n%s", protocol)
	}
	if !strings.Contains(protocol, "graphit_wiki_source") || !strings.Contains(protocol, `wiki: "memory"`) {
		t.Fatalf("protocol does not require reading selected memory pages:\n%s", protocol)
	}
}

func TestRenderNativeFormats(t *testing.T) {
	t.Parallel()

	for _, format := range []string{FormatSessionStart, FormatAdditionalContext} {
		format := format
		t.Run(format, func(t *testing.T) {
			t.Parallel()
			payload, err := Render(format, nil)
			if err != nil {
				t.Fatal(err)
			}
			if !json.Valid(payload) || !strings.Contains(string(payload), "graphit_memory_mandatory") {
				t.Fatalf("invalid %s payload: %s", format, payload)
			}
		})
	}
}

func TestRenderAntigravityOnlyInjectsBeforeTheFirstInvocation(t *testing.T) {
	t.Parallel()

	first, err := Render(FormatFirstInvocation, []byte(`{"invocationNum":0}`))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(first), "ephemeralMessage") || !strings.Contains(string(first), "graphit_memory_mandatory") {
		t.Fatalf("first invocation did not inject the protocol: %s", first)
	}

	later, err := Render(FormatFirstInvocation, []byte(`{"invocationNum":1}`))
	if err != nil {
		t.Fatal(err)
	}
	if string(later) != "{}" {
		t.Fatalf("later invocation must not inject again: %s", later)
	}
}

func TestRenderRejectsInvalidInputAndUnknownAdapters(t *testing.T) {
	t.Parallel()

	if _, err := Render(FormatFirstInvocation, []byte(`{}`)); err == nil {
		t.Fatal("expected missing invocationNum to fail")
	}
	if _, err := Render("unknown", nil); err == nil {
		t.Fatal("expected unknown format to fail")
	}
}
