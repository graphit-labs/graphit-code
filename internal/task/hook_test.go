package task

import "testing"

func TestAgentIDFromHookUsesDeterministicIdentityPriority(t *testing.T) {
	payload := []byte(`{
		"thread_id": "thread-root",
		"z": {"session_id": "session-z"},
		"a": {"agent_id": "agent-a"}
	}`)
	want := AgentIDForSession("agent-a")
	for range 100 {
		if got := AgentIDFromHook(payload); got != want {
			t.Fatalf("AgentIDFromHook() = %q, want %q", got, want)
		}
	}
}

func TestAgentIDFromHookDoesNotGuessUnknownPayload(t *testing.T) {
	if got := AgentIDFromHook([]byte(`{"cwd":"/tmp/project"}`)); got != "" {
		t.Fatalf("AgentIDFromHook() = %q, want empty", got)
	}
}

func TestAgentIDFromHookAcceptsNativeIDAliases(t *testing.T) {
	for _, payload := range []string{
		`{"sessionID":"native-session"}`,
		`{"threadID":"native-session"}`,
		`{"event":{"properties":{"sessionID":"native-session"}}}`,
	} {
		if got, want := AgentIDFromHook([]byte(payload)), AgentIDForSession("native-session"); got != want {
			t.Fatalf("identity from %s = %q, want %q", payload, got, want)
		}
	}
	for _, payload := range []string{`{"sessionID":null}`, `{"sessionID":" "}`, `{"info":{"id":"ambiguous"}}`, `{`} {
		if got := AgentIDFromHook([]byte(payload)); got != "" {
			t.Fatalf("unsafe identity inferred from %s: %q", payload, got)
		}
	}
}
