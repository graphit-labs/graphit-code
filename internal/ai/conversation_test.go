package ai

import (
	"context"
	"errors"
	"sync"
	"testing"
	"time"
)

type conversationTestClient struct {
	mu       sync.Mutex
	calls    []conversationTestCall
	nextID   string
	nextIDs  []string
	failNext bool
	active   int
	maxLive  int
}

type conversationTestCall struct {
	workDir   string
	sessionID string
}

func (c *conversationTestClient) Complete(context.Context, string, string) (string, error) {
	return "stateless", nil
}

func (c *conversationTestClient) CompleteWithSession(ctx context.Context, sessionID, systemPrompt, userPrompt string) (string, string, error) {
	return c.CompleteWithSessionInDir(ctx, "", sessionID, systemPrompt, userPrompt)
}

func (c *conversationTestClient) CompleteWithSessionInDir(ctx context.Context, workDir, sessionID, systemPrompt, userPrompt string) (string, string, error) {
	c.mu.Lock()
	c.calls = append(c.calls, conversationTestCall{workDir: workDir, sessionID: sessionID})
	c.active++
	if c.active > c.maxLive {
		c.maxLive = c.active
	}
	fail := c.failNext
	c.failNext = false
	nextID := c.nextID
	if len(c.nextIDs) > 0 {
		nextID = c.nextIDs[0]
		c.nextIDs = c.nextIDs[1:]
	}
	c.mu.Unlock()

	select {
	case <-ctx.Done():
		c.mu.Lock()
		c.active--
		c.mu.Unlock()
		return "", "", ctx.Err()
	case <-time.After(5 * time.Millisecond):
	}

	c.mu.Lock()
	c.active--
	c.mu.Unlock()
	if fail {
		return "", "", errors.New("resume failed")
	}
	if sessionID == "" {
		sessionID = nextID
	}
	return "ok", sessionID, nil
}

func (c *conversationTestClient) SupportsSession() bool { return true }
func (c *conversationTestClient) AgentCLI() string      { return "test-agent" }

func TestConversationReusesSessionAndWorkingDirectory(t *testing.T) {
	client := &conversationTestClient{nextID: "session-1"}
	conversation := NewConversation(client, t.TempDir())
	if _, err := conversation.Complete(context.Background(), "system", "first"); err != nil {
		t.Fatal(err)
	}
	if _, err := conversation.Complete(context.Background(), "system", "second"); err != nil {
		t.Fatal(err)
	}
	if len(client.calls) != 2 || client.calls[0].sessionID != "" || client.calls[1].sessionID != "session-1" {
		t.Fatalf("calls = %#v", client.calls)
	}
	if client.calls[0].workDir != conversation.WorkDir() || client.calls[1].workDir != conversation.WorkDir() {
		t.Fatalf("working directories = %#v, want %q", client.calls, conversation.WorkDir())
	}
}

func TestConversationSerializesConcurrentTurns(t *testing.T) {
	client := &conversationTestClient{nextID: "session-1"}
	conversation := NewConversation(client, t.TempDir())
	var wg sync.WaitGroup
	for range 4 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			_, _ = conversation.Complete(context.Background(), "", "turn")
		}()
	}
	wg.Wait()
	if client.maxLive != 1 {
		t.Fatalf("maximum concurrent turns = %d, want 1", client.maxLive)
	}
}

func TestConversationInvalidatesFailedSessionAndRejectsDifferentAgent(t *testing.T) {
	client := &conversationTestClient{nextID: "new-session", failNext: true}
	conversation := ResumeConversation(client, t.TempDir(), "old-session", "test-agent")
	if _, err := conversation.Complete(context.Background(), "", "turn"); err == nil {
		t.Fatal("expected resume failure")
	}
	if conversation.SessionID() != "" {
		t.Fatalf("failed session was not invalidated: %q", conversation.SessionID())
	}

	other := ResumeConversation(client, t.TempDir(), "foreign-session", "other-agent")
	if other.SessionID() != "" {
		t.Fatalf("foreign agent session was accepted: %q", other.SessionID())
	}
	unknown := ResumeConversation(client, t.TempDir(), "unbound-session", "")
	if unknown.SessionID() != "" {
		t.Fatalf("session without an agent binding was accepted: %q", unknown.SessionID())
	}
}

func TestConversationsDoNotShareSessionsAcrossWorkingDirectories(t *testing.T) {
	client := &conversationTestClient{nextIDs: []string{"project-a-session", "project-b-session"}}
	projectA := t.TempDir()
	projectB := t.TempDir()
	conversationA := NewConversation(client, projectA)
	conversationB := NewConversation(client, projectB)

	if _, err := conversationA.Complete(context.Background(), "", "first"); err != nil {
		t.Fatal(err)
	}
	if _, err := conversationB.Complete(context.Background(), "", "first"); err != nil {
		t.Fatal(err)
	}
	if conversationA.SessionID() == conversationB.SessionID() {
		t.Fatalf("different projects shared session %q", conversationA.SessionID())
	}
	if client.calls[0].workDir != projectA || client.calls[1].workDir != projectB {
		t.Fatalf("working directories = %#v, want %q then %q", client.calls, projectA, projectB)
	}
}

func TestConversationCancellationInvalidatesResumedSession(t *testing.T) {
	client := &conversationTestClient{}
	conversation := ResumeConversation(client, t.TempDir(), "existing-session", "test-agent")
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	if _, err := conversation.Complete(ctx, "", "cancelled turn"); !errors.Is(err, context.Canceled) {
		t.Fatalf("error = %v, want context.Canceled", err)
	}
	if conversation.SessionID() != "" {
		t.Fatalf("cancelled session was not invalidated: %q", conversation.SessionID())
	}
	if len(client.calls) != 1 {
		t.Fatalf("calls = %d, want exactly the cancelled turn", len(client.calls))
	}
}
