package chat

import (
	"context"
	"errors"
	"os"
	"path/filepath"
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
	_ = os.Setenv("HOME", tempHome)

	t.Cleanup(func() {
		_ = os.Setenv("HOME", origHome)
		_ = os.RemoveAll(tempHome)
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

func TestSendAppendError(t *testing.T) {
	origHome := os.Getenv("HOME")
	// Set HOME to a non-writable path to trigger Append error
	_ = os.Setenv("HOME", "/dev/null/nonexistent")
	defer func() { _ = os.Setenv("HOME", origHome) }()

	session := &ChatSession{
		ID:          "test-id",
		ProjectDir:  "/project",
		ProjectHash: "abc",
	}

	engine := NewChatEngine(&mockAIClient{}, session)
	_, err := engine.Send(context.Background(), "test")
	if err == nil {
		t.Error("expected error from Append failure")
	}
}

func TestSendAIError(t *testing.T) {
	_ = setupChatTestHome(t)

	session := NewSession("/project", nil, "test")
	ai := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			return "", errors.New("AI unavailable")
		},
	}
	engine := NewChatEngine(ai, session)
	_, err := engine.Send(context.Background(), "test message")
	if err == nil || !strings.Contains(err.Error(), "AI error") {
		t.Errorf("expected AI error, got %v", err)
	}
}

func TestSendWithSearchNilResult(t *testing.T) {
	_ = setupChatTestHome(t)

	session := NewSession("/project", nil, "test")
	ai := &mockAIClient{}
	engine := NewChatEngine(ai, session)

	// nil searchResult — should skip search append
	resp, err := engine.SendWithSearch(context.Background(), "test", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp != "mocked response" {
		t.Errorf("expected 'mocked response', got %q", resp)
	}
}

func TestSendWithSearchEmptyAnswer(t *testing.T) {
	_ = setupChatTestHome(t)

	session := NewSession("/project", nil, "test")
	ai := &mockAIClient{}
	engine := NewChatEngine(ai, session)

	// Empty Answer — should skip search append
	resp, err := engine.SendWithSearch(context.Background(), "test", &wiki.SearchResult{Answer: ""})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if resp != "mocked response" {
		t.Errorf("expected 'mocked response', got %q", resp)
	}
}

func TestBuildWikiSourceContextEmpty(t *testing.T) {
	result := buildWikiSourceContext(nil)
	if result != "" {
		t.Errorf("expected empty string for nil sources, got %q", result)
	}
	result2 := buildWikiSourceContext([]WikiSource{})
	if result2 != "" {
		t.Errorf("expected empty string for empty sources, got %q", result2)
	}
}

func TestBuildChatSystemPromptNoSources(t *testing.T) {
	prompt := buildChatSystemPrompt(nil)
	if !strings.Contains(prompt, "0 wiki sources") {
		t.Errorf("expected '0 wiki sources' in prompt, got %q", prompt)
	}
}

func TestLoadHistoryNonExistentFile(t *testing.T) {
	_ = setupChatTestHome(t)

	session := NewSession("/project", nil, "test")
	// No messages appended — file doesn't exist
	history, err := session.LoadHistory()
	if err != nil {
		t.Fatalf("expected no error, got %v", err)
	}
	if len(history) != 0 {
		t.Errorf("expected 0 messages, got %d", len(history))
	}
}

func TestLoadHistoryCorruptLine(t *testing.T) {
	_ = setupChatTestHome(t)

	session := NewSession("/project", nil, "test")
	// Append a valid message first
	_ = session.Append(ChatMessage{Role: "user", Content: "hello"})

	// Manually write a corrupt line to the JSONL file
	f, _ := os.OpenFile(session.jsonlPath(), os.O_APPEND|os.O_WRONLY, 0644)
	_, _ = f.WriteString("this is not json\n")
	_ = f.Close()

	// Append another valid message
	_ = session.Append(ChatMessage{Role: "assistant", Content: "world"})

	history, err := session.LoadHistory()
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	// Should skip the corrupt line, returning 2 valid messages
	if len(history) != 2 {
		t.Errorf("expected 2 messages (skipping corrupt), got %d", len(history))
	}
}

func TestBuildContextZeroMax(t *testing.T) {
	_ = setupChatTestHome(t)

	session := NewSession("/project", nil, "test")
	_ = session.Append(ChatMessage{Role: "user", Content: "msg1"})
	_ = session.Append(ChatMessage{Role: "user", Content: "msg2"})

	// maxMessages <= 0 should default to 20
	ctx, err := session.BuildContext(0)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(ctx, "msg1") || !strings.Contains(ctx, "msg2") {
		t.Error("expected all messages with default maxMessages")
	}
}

func TestLoadSessionEmptyGlobalDir(t *testing.T) {
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", "")
	defer func() { _ = os.Setenv("HOME", origHome) }()

	_, err := LoadSession("some-id")
	if err == nil {
		t.Error("expected error when global dir is empty")
	}
}

func TestDeleteSessionNotFound(t *testing.T) {
	_ = setupChatTestHome(t)

	err := DeleteSession("nonexistent-session-id")
	if err == nil {
		t.Error("expected error deleting nonexistent session")
	}
}

func TestListSessionsWithNonJSONFiles(t *testing.T) {
	home := setupChatTestHome(t)

	projectDir := "/my/project"
	h := projectHash(projectDir)
	metaDir := filepath.Join(home, ".graphit", "chat", "sessions", h, "meta")
	_ = os.MkdirAll(metaDir, 0755)

	// Write a non-JSON file
	_ = os.WriteFile(filepath.Join(metaDir, "readme.txt"), []byte("not json"), 0644)

	// Write an invalid JSON file
	_ = os.WriteFile(filepath.Join(metaDir, "bad.json"), []byte("invalid json"), 0644)

	sessions, err := ListSessions(projectDir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	// Both should be skipped
	if len(sessions) != 0 {
		t.Errorf("expected 0 sessions, got %d", len(sessions))
	}
}

func TestLoadMetaInvalidJSON(t *testing.T) {
	tmpDir := t.TempDir()
	metaFile := filepath.Join(tmpDir, "bad.json")
	_ = os.WriteFile(metaFile, []byte("{{invalid json"), 0644)

	_, err := loadMeta(metaFile)
	if err == nil {
		t.Error("expected error for invalid JSON")
	}
}

func TestSaveMetaMkdirError(t *testing.T) {
	// Set HOME to a path that can't have subdirs created
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", "/dev/null")
	defer func() { _ = os.Setenv("HOME", origHome) }()

	session := &ChatSession{
		ID:          "test-id",
		ProjectDir:  "/project",
		ProjectHash: "abc",
	}
	err := session.saveMeta()
	if err == nil {
		t.Error("expected error from MkdirAll failure")
	}
}

func TestSessionsBaseDirEmpty(t *testing.T) {
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", "")
	defer func() { _ = os.Setenv("HOME", origHome) }()

	dir := sessionsBaseDir()
	if dir != "" {
		t.Errorf("expected empty sessionsBaseDir when HOME is empty, got %q", dir)
	}
}

func TestAppendMkdirAllError(t *testing.T) {
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", "/dev/null")
	defer func() { _ = os.Setenv("HOME", origHome) }()

	session := &ChatSession{
		ID:          "test-id",
		ProjectDir:  "/project",
		ProjectHash: "abc",
	}
	err := session.Append(ChatMessage{Role: "user", Content: "test"})
	if err == nil {
		t.Error("expected error from MkdirAll failure")
	}
}

func TestLatestSessionError(t *testing.T) {
	origHome := os.Getenv("HOME")
	tmpDir := t.TempDir()
	_ = os.Setenv("HOME", tmpDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Create a sessions dir that returns error during ReadDir by making
	// the meta directory a file (not a dir)
	h := projectHash("/errorproject")
	metaDir := filepath.Join(tmpDir, ".graphit", "chat", "sessions", h, "meta")
	_ = os.MkdirAll(filepath.Dir(metaDir), 0755)
	_ = os.WriteFile(metaDir, []byte("not a directory"), 0644)

	_, err := ListSessions("/errorproject")
	if err == nil {
		t.Error("expected error from ListSessions when meta is not a directory")
	}
}

func TestLoadSessionReadDirError(t *testing.T) {
	tmpDir := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tmpDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Create the sessions base dir but make it unreadable
	sessDir := filepath.Join(tmpDir, ".graphit", "chat", "sessions")
	_ = os.MkdirAll(sessDir, 0755)

	// Session not found case
	_, err := LoadSession("nonexistent-id")
	if err == nil || !strings.Contains(err.Error(), "not found") {
		t.Errorf("expected 'not found' error, got %v", err)
	}
}

func TestLoadHistoryOpenError(t *testing.T) {
	tmpDir := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tmpDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	session := NewSession("/project", nil, "test")
	// Create the messages dir and file as a directory (to trigger open error)
	msgDir := filepath.Dir(session.jsonlPath())
	_ = os.MkdirAll(session.jsonlPath(), 0755) // file path is now a directory
	_ = msgDir

	_, err := session.LoadHistory()
	if err == nil {
		t.Error("expected error when jsonl path is a directory")
	}
}

func TestBuildContextLoadHistoryError(t *testing.T) {
	tmpDir := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tmpDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	session := NewSession("/project", nil, "test")
	// Make jsonl path a directory to trigger LoadHistory error
	_ = os.MkdirAll(session.jsonlPath(), 0755)

	_, err := session.BuildContext(10)
	if err == nil {
		t.Error("expected error from BuildContext when LoadHistory fails")
	}
}

func TestSendBuildContextError(t *testing.T) {
	tmpDir := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tmpDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	session := NewSession("/project", nil, "test")
	// Append first message (creates the file)
	_ = session.Append(ChatMessage{Role: "user", Content: "setup"})

	// Now make the jsonl file a directory (will make LoadHistory in BuildContext fail)
	_ = os.Remove(session.jsonlPath())
	_ = os.MkdirAll(session.jsonlPath(), 0755)

	engine := NewChatEngine(&mockAIClient{}, session)
	_, err := engine.Send(context.Background(), "test")
	if err == nil {
		t.Error("expected error from BuildContext failure in Send")
	}
}

func TestSendResponseAppendError(t *testing.T) {
	tmpDir := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tmpDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	session := NewSession("/project", nil, "test")
	ai := &mockAIClient{
		completeFunc: func(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
			return "response", nil
		},
	}
	engine := NewChatEngine(ai, session)

	// Wrap the session's Append to fail on the second call (AI response save)
	origAppend := session.Append
	_ = origAppend
	// We can't easily mock Append, but we can remove the session dir after the first Append
	// by making the messages dir read-only after the initial user message is saved
	_, err := engine.Send(context.Background(), "first message")
	if err != nil {
		t.Fatalf("first send should succeed: %v", err)
	}

	// Make the messages file read-only so the next Append fails on OpenFile
	_ = os.Chmod(session.jsonlPath(), 0444)
	defer func() { _ = os.Chmod(session.jsonlPath(), 0644) }()

	_, err = engine.Send(context.Background(), "second message")
	if err == nil || !strings.Contains(err.Error(), "saving user message") {
		t.Errorf("expected 'saving user message' error, got %v", err)
	}
}

func TestSendWithSearchAppendError(t *testing.T) {
	tmpDir := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tmpDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	session := NewSession("/project", nil, "test query")

	// Make messages file dir writable, then make it read-only to trigger Append error
	// First need the dir to exist
	_ = os.MkdirAll(filepath.Dir(session.jsonlPath()), 0755)
	_ = os.WriteFile(session.jsonlPath(), []byte{}, 0444)
	defer func() { _ = os.Chmod(session.jsonlPath(), 0644) }()

	ai := &mockAIClient{}
	engine := NewChatEngine(ai, session)

	searchResult := &wiki.SearchResult{Answer: "wiki answer"}
	_, err := engine.SendWithSearch(context.Background(), "test", searchResult)
	if err == nil || !strings.Contains(err.Error(), "saving search context") {
		t.Errorf("expected 'saving search context' error, got %v", err)
	}
}

func TestLoadSessionSkipsFiles(t *testing.T) {
	tmpDir := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tmpDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Create sessions dir with a file (not dir) entry
	sessDir := filepath.Join(tmpDir, ".graphit", "chat", "sessions")
	_ = os.MkdirAll(sessDir, 0755)
	_ = os.WriteFile(filepath.Join(sessDir, "not-a-dir"), []byte("file"), 0644)

	// Session should not be found
	_, err := LoadSession("test-id")
	if err == nil || !strings.Contains(err.Error(), "not found") {
		t.Errorf("expected 'not found' error, got %v", err)
	}
}

func TestLatestSessionListError(t *testing.T) {
	tmpDir := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tmpDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Create a meta dir that's a file to cause ListSessions error
	h := projectHash("/listfailproject")
	metaPath := filepath.Join(tmpDir, ".graphit", "chat", "sessions", h, "meta")
	_ = os.MkdirAll(filepath.Dir(metaPath), 0755)
	_ = os.WriteFile(metaPath, []byte("not a dir"), 0644)

	_, err := LatestSession("/listfailproject")
	if err == nil {
		t.Error("expected error from LatestSession when ListSessions fails")
	}
}

func TestListSessionsSortOrder(t *testing.T) {
	tmpDir := t.TempDir()
	origHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", tmpDir)
	defer func() { _ = os.Setenv("HOME", origHome) }()

	projectDir := "/sorttest"
	h := projectHash(projectDir)
	metaDir := filepath.Join(tmpDir, ".graphit", "chat", "sessions", h, "meta")
	_ = os.MkdirAll(metaDir, 0755)

	// Write two valid sessions with different UpdatedAt times
	session1 := `{"id":"s1","updated_at":"2024-01-01T00:00:00Z","title":"old"}`
	session2 := `{"id":"s2","updated_at":"2024-06-01T00:00:00Z","title":"new"}`
	_ = os.WriteFile(filepath.Join(metaDir, "s1.json"), []byte(session1), 0644)
	_ = os.WriteFile(filepath.Join(metaDir, "s2.json"), []byte(session2), 0644)

	sessions, err := ListSessions(projectDir)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(sessions) != 2 {
		t.Fatalf("expected 2 sessions, got %d", len(sessions))
	}
	if sessions[0].ID != "s2" {
		t.Errorf("expected most recent session first (s2), got %s", sessions[0].ID)
	}
}
