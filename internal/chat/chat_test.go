package chat

import (
	"context"
	"errors"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type mockAIClient struct {
	completeFunc func(ctx context.Context, systemPrompt, userPrompt string) (string, error)
}

func (m *mockAIClient) Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
	if m.completeFunc != nil {
		return m.completeFunc(ctx, systemPrompt, userPrompt)
	}
	return "mocked response", nil
}

func setupChatTestHome(t *testing.T) string {
	tempHome, err := os.MkdirTemp("", "graphit-chat-test-*")
	if err != nil {
		t.Fatalf("failed to create temp home: %v", err)
	}
	origHome := os.Getenv("HOME")
	os.Setenv("HOME", tempHome)

	t.Cleanup(func() {
		os.Setenv("HOME", origHome)
		os.RemoveAll(tempHome)
	})

	return tempHome
}

func TestChatEngineAndSession(t *testing.T) {
	_ = setupChatTestHome(t)

	sources := []WikiSource{
		{ID: "wiki-1", Label: "Wiki One", Dir: "/dir/wiki1"},
	}

	// 1. Create Session
	session := NewSession("/project/dir", sources, "How does X work?")
	if session == nil {
		t.Fatal("expected non-nil ChatSession")
	}
	if session.Title != "How does X work?" {
		t.Errorf("expected title 'How does X work?', got %q", session.Title)
	}

	// 2. Chat Engine setup
	mockAI := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			if !strings.Contains(systemPrompt, "Wiki One") {
				return "", errors.New("expected Wiki One in system prompt")
			}
			if !strings.Contains(userPrompt, "How does X work?") {
				return "", errors.New("expected initial question in user prompt")
			}
			return "Here is how X works.", nil
		},
	}

	engine := NewChatEngine(mockAI, session)
	if engine.Session() != session {
		t.Error("engine.Session() returned unexpected session")
	}

	// 3. Send message
	ctx := context.Background()
	resp, err := engine.Send(ctx, "How does X work?")
	if err != nil {
		t.Fatalf("failed to send message: %v", err)
	}
	if resp != "Here is how X works." {
		t.Errorf("expected response 'Here is how X works.', got %q", resp)
	}

	// 4. Send message with search
	mockAI.completeFunc = func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
		return "Based on search, Y works like this.", nil
	}
	searchRes := &wiki.SearchResult{
		Answer: "Wiki search answer contents",
	}
	respSearch, err := engine.SendWithSearch(ctx, "Explain Y", searchRes)
	if err != nil {
		t.Fatalf("failed to send with search: %v", err)
	}
	if respSearch != "Based on search, Y works like this." {
		t.Errorf("unexpected response: %q", respSearch)
	}

	// 5. Load and verify history
	history, err := session.LoadHistory()
	if err != nil {
		t.Fatalf("failed to load history: %v", err)
	}
	// We expect: user (How does X work), assistant (Here is how X works), system (Wiki search result), user (Explain Y), assistant (Based on search, Y works like this)
	if len(history) != 5 {
		t.Errorf("expected 5 messages in history, got %d", len(history))
	}
	if history[0].Role != "user" || history[1].Role != "assistant" {
		t.Errorf("unexpected history roles: %v", history)
	}

	// 6. Build Context
	ctxStr, err := session.BuildContext(2)
	if err != nil {
		t.Fatalf("failed to build context: %v", err)
	}
	if !strings.Contains(ctxStr, "Explain Y") || strings.Contains(ctxStr, "How does X work?") {
		t.Errorf("expected last 2 messages in context output, got: %s", ctxStr)
	}

	// 7. List sessions
	sessions, err := ListSessions("/project/dir")
	if err != nil {
		t.Fatalf("failed to list sessions: %v", err)
	}
	if len(sessions) != 1 || sessions[0].ID != session.ID {
		t.Errorf("expected 1 session with ID %s, got %v", session.ID, sessions)
	}

	// 8. Latest Session
	latest, err := LatestSession("/project/dir")
	if err != nil {
		t.Fatalf("failed to get latest session: %v", err)
	}
	if latest.ID != session.ID {
		t.Errorf("expected latest session ID %s, got %s", session.ID, latest.ID)
	}

	// 9. Load Session
	loaded, err := LoadSession(session.ID)
	if err != nil {
		t.Fatalf("failed to load session by ID: %v", err)
	}
	if loaded.ID != session.ID {
		t.Errorf("expected loaded session ID %s, got %s", session.ID, loaded.ID)
	}

	// 10. Delete session
	err = DeleteSession(session.ID)
	if err != nil {
		t.Fatalf("failed to delete session: %v", err)
	}

	// Verify deleted
	_, err = LoadSession(session.ID)
	if err == nil {
		t.Error("expected error loading deleted session")
	}
}

func TestChatSessionEdgeCases(t *testing.T) {
	_ = setupChatTestHome(t)

	// Long query title truncation
	longQuery := strings.Repeat("A", 100)
	s := NewSession("/project", nil, longQuery)
	if len(s.Title) != 83 || !strings.HasSuffix(s.Title, "…") {
		t.Errorf("expected truncated title, got %q (length %d)", s.Title, len(s.Title))
	}

	// Empty list for nonexistent project
	list, err := ListSessions("/nonexistent")
	if err != nil {
		t.Errorf("expected no error listing nonexistent project sessions, got %v", err)
	}
	if len(list) != 0 {
		t.Errorf("expected 0 sessions, got %d", len(list))
	}

	// LatestSession error for empty sessions
	_, err = LatestSession("/nonexistent")
	if err == nil {
		t.Error("expected error getting latest session for nonexistent project")
	}

	// Append with manual timestamp and tokens
	msg := ChatMessage{
		Role:      "user",
		Content:   "msg",
		Timestamp: time.Now().UTC(),
		Tokens:    10,
	}
	err = s.Append(msg)
	if err != nil {
		t.Errorf("failed to append message: %v", err)
	}
}
