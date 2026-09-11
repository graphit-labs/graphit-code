package ai

import (
	"context"
	"os"
	"path/filepath"
	"sync"
)

type WorkingDirectoryClient interface {
	Client
	CompleteInDir(ctx context.Context, workingDir, systemPrompt, userPrompt string) (string, error)
}

type WorkingDirectorySessionClient interface {
	SessionClient
	CompleteWithSessionInDir(ctx context.Context, workingDir, sessionID, systemPrompt, userPrompt string) (response string, newSessionID string, err error)
}

type identifiedClient interface {
	AgentCLI() string
}

// Conversation serializes turns that belong to one logical interaction and
// keeps the agent CLI's native session bound to one working directory.
type Conversation struct {
	client  Client
	workDir string
	agent   string

	mu        sync.Mutex
	sessionID string
}

func NewConversation(client Client, workDir string) *Conversation {
	return ResumeConversation(client, workDir, "", "")
}

func ResumeConversation(client Client, workDir, sessionID, agentCLI string) *Conversation {
	agent := ""
	if identified, ok := client.(identifiedClient); ok {
		agent = identified.AgentCLI()
	}
	if sessionID != "" && (agentCLI == "" || agent == "" || agent != agentCLI) {
		sessionID = ""
	}
	return &Conversation{
		client:    client,
		workDir:   normalizeWorkDir(workDir),
		agent:     agent,
		sessionID: sessionID,
	}
}

func (c *Conversation) Complete(ctx context.Context, systemPrompt, userPrompt string) (string, error) {
	c.mu.Lock()
	defer c.mu.Unlock()

	if client, ok := c.client.(WorkingDirectorySessionClient); ok && client.SupportsSession() {
		response, sessionID, err := client.CompleteWithSessionInDir(ctx, c.workDir, c.sessionID, systemPrompt, userPrompt)
		if err != nil {
			c.sessionID = ""
			return "", err
		}
		c.sessionID = sessionID
		return response, nil
	}
	if client, ok := c.client.(SessionClient); ok && client.SupportsSession() {
		response, sessionID, err := client.CompleteWithSession(ctx, c.sessionID, systemPrompt, userPrompt)
		if err != nil {
			c.sessionID = ""
			return "", err
		}
		c.sessionID = sessionID
		return response, nil
	}
	if client, ok := c.client.(WorkingDirectoryClient); ok {
		return client.CompleteInDir(ctx, c.workDir, systemPrompt, userPrompt)
	}
	return c.client.Complete(ctx, systemPrompt, userPrompt)
}

func (c *Conversation) SessionID() string {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.sessionID
}

func (c *Conversation) AgentCLI() string { return c.agent }

func (c *Conversation) WorkDir() string { return c.workDir }

func normalizeWorkDir(workDir string) string {
	if workDir == "" {
		workDir, _ = os.Getwd()
	}
	if abs, err := filepath.Abs(filepath.Clean(workDir)); err == nil {
		workDir = abs
	}
	if resolved, err := filepath.EvalSymlinks(workDir); err == nil {
		workDir = resolved
	}
	return workDir
}
