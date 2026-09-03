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
