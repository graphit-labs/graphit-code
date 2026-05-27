package chat

import (
	"bufio"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/rand"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/oklog/ulid/v2"
)

type WikiSource struct {
	ID    string `json:"id"`
	Label string `json:"label"`
	Dir   string `json:"dir"`
}

type ChatMessage struct {
	Role      string    `json:"role"`
	Content   string    `json:"content"`
	Timestamp time.Time `json:"timestamp"`
	Tokens    int       `json:"tokens,omitempty"`
}

type ChatSession struct {
	ID           string       `json:"id"`
	ProjectDir   string       `json:"project_dir"`
	ProjectHash  string       `json:"project_hash"`
	CreatedAt    time.Time    `json:"created_at"`
	UpdatedAt    time.Time    `json:"updated_at"`
	Title        string       `json:"title"`
	WikiSources  []WikiSource `json:"wiki_sources"`
	InitialQuery string       `json:"initial_query"`
	MessageCount int          `json:"message_count"`
}

func NewSession(projectDir string, sources []WikiSource, query string) *ChatSession {
	entropy := ulid.Monotonic(rand.New(rand.NewSource(time.Now().UnixNano())), 0)
	id := ulid.MustNew(ulid.Timestamp(time.Now()), entropy).String()

	now := time.Now().UTC()
	title := query
	if len(title) > 80 {
		title = title[:80] + "…"
	}

	return &ChatSession{
		ID:           id,
		ProjectDir:   projectDir,
		ProjectHash:  projectHash(projectDir),
		CreatedAt:    now,
		UpdatedAt:    now,
		Title:        title,
		WikiSources:  sources,
		InitialQuery: query,
	}
}

func (s *ChatSession) Append(msg ChatMessage) error {
	if msg.Timestamp.IsZero() {
		msg.Timestamp = time.Now().UTC()
	}
	if msg.Tokens == 0 {
		msg.Tokens = len(msg.Content) / 4
	}

	if err := os.MkdirAll(filepath.Dir(s.jsonlPath()), 0o755); err != nil {
		return fmt.Errorf("creating session dir: %w", err)
	}

	f, err := os.OpenFile(s.jsonlPath(), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return fmt.Errorf("opening session file: %w", err)
	}
	defer func() { _ = f.Close() }()

	data, err := json.Marshal(msg)
	if err != nil {
		return fmt.Errorf("marshalling message: %w", err)
	}
	if _, err := f.Write(append(data, '\n')); err != nil {
		return fmt.Errorf("writing message: %w", err)
	}

	s.UpdatedAt = msg.Timestamp
	s.MessageCount++
	return s.saveMeta()
}

func (s *ChatSession) LoadHistory() ([]ChatMessage, error) {
	f, err := os.Open(s.jsonlPath())
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("opening session file: %w", err)
	}
	defer func() { _ = f.Close() }()

	var messages []ChatMessage
	scanner := bufio.NewScanner(f)

	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)

	for scanner.Scan() {
		var msg ChatMessage
		if err := json.Unmarshal(scanner.Bytes(), &msg); err != nil {
			continue
		}
		messages = append(messages, msg)
	}
	return messages, scanner.Err()
}

func (s *ChatSession) BuildContext(maxMessages int) (string, error) {
	if maxMessages <= 0 {
		maxMessages = 20
	}

	messages, err := s.LoadHistory()
	if err != nil {
		return "", err
	}

	if len(messages) > maxMessages {
		messages = messages[len(messages)-maxMessages:]
	}

	var b strings.Builder
	b.WriteString("=== Conversation History ===\n")
	for _, msg := range messages {
		_, _ = fmt.Fprintf(&b, "[%s] %s\n\n", msg.Role, msg.Content)
	}
	return b.String(), nil
}

func LoadSession(sessionID string) (*ChatSession, error) {
	baseDir := sessionsBaseDir()
	if baseDir == "" {
		return nil, fmt.Errorf("cannot resolve global dir")
	}

	entries, err := os.ReadDir(baseDir)
	if err != nil {
		return nil, fmt.Errorf("reading sessions dir: %w", err)
	}

	for _, e := range entries {
		if !e.IsDir() {
			continue
		}
		metaPath := filepath.Join(baseDir, e.Name(), "meta", sessionID+".json")
		session, err := loadMeta(metaPath)
		if err == nil {
			return session, nil
		}
	}

	return nil, fmt.Errorf("session %q not found", sessionID)
}

func ListSessions(projectDir string) ([]*ChatSession, error) {
	metaDir := filepath.Join(sessionsBaseDir(), projectHash(projectDir), "meta")
	entries, err := os.ReadDir(metaDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("reading sessions: %w", err)
	}

	var sessions []*ChatSession
	for _, e := range entries {
		if !strings.HasSuffix(e.Name(), ".json") {
			continue
		}
		session, err := loadMeta(filepath.Join(metaDir, e.Name()))
		if err != nil {
			continue
		}
		sessions = append(sessions, session)
	}

	sort.Slice(sessions, func(i, j int) bool {
		return sessions[i].UpdatedAt.After(sessions[j].UpdatedAt)
	})
	return sessions, nil
}

func LatestSession(projectDir string) (*ChatSession, error) {
	sessions, err := ListSessions(projectDir)
	if err != nil {
		return nil, err
	}
	if len(sessions) == 0 {
		return nil, fmt.Errorf("no sessions found for this project")
	}
	return sessions[0], nil
}

func DeleteSession(sessionID string) error {
	session, err := LoadSession(sessionID)
	if err != nil {
		return err
	}

	dir := session.sessionDir()
	_ = os.Remove(filepath.Join(dir, "meta", sessionID+".json"))
	_ = os.Remove(filepath.Join(dir, "messages", sessionID+".jsonl"))
	return nil
}

func (s *ChatSession) sessionDir() string {
	return filepath.Join(sessionsBaseDir(), s.ProjectHash)
}

func (s *ChatSession) jsonlPath() string {
	return filepath.Join(s.sessionDir(), "messages", s.ID+".jsonl")
}

func (s *ChatSession) metaPath() string {
	return filepath.Join(s.sessionDir(), "meta", s.ID+".json")
}

func (s *ChatSession) saveMeta() error {
	data, err := json.MarshalIndent(s, "", "  ")
	if err != nil {
		return fmt.Errorf("marshalling session meta: %w", err)
	}
	if err := os.MkdirAll(filepath.Dir(s.metaPath()), 0o755); err != nil {
		return fmt.Errorf("creating meta dir: %w", err)
	}
	return os.WriteFile(s.metaPath(), data, 0o644)
}

func loadMeta(path string) (*ChatSession, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var session ChatSession
	if err := json.Unmarshal(data, &session); err != nil {
		return nil, err
	}
	return &session, nil
}

func sessionsBaseDir() string {
	globalDir := brand.GlobalDir()
	if globalDir == "" {
		return ""
	}
	return filepath.Join(globalDir, "chat", "sessions")
}

func projectHash(projectDir string) string {
	h := sha256.Sum256([]byte(projectDir))
	return hex.EncodeToString(h[:8])
}
