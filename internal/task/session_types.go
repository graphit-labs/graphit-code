package task

import "github.com/graphit-labs/graphit-code/internal/relations"

// Session is the durable user-intent and coordination boundary, independent of
// a host conversation ID and of the claims held by its task workers.
type Session struct {
	References         *[]relations.Ref `json:"references,omitempty"`
	ID                 string           `json:"id"`
	ProjectID          string           `json:"project_id"`
	IdempotencyKey     string           `json:"idempotency_key"`
	Title              string           `json:"title"`
	Description        string           `json:"description"`
	Strategy           string           `json:"strategy"`
	Status             Status           `json:"status"`
	Owner              string           `json:"owner,omitempty"`
	ClaimToken         string           `json:"claim_token,omitempty"`
	ClaimEpoch         int64            `json:"claim_epoch"`
	ClaimedAt          string           `json:"claimed_at,omitempty"`
	LeaseExpiresAt     string           `json:"lease_expires_at,omitempty"`
	HeartbeatAt        string           `json:"heartbeat_at,omitempty"`
	CheckpointSequence int64            `json:"checkpoint_sequence"`
	ProgressSummary    string           `json:"progress_summary,omitempty"`
	NextStep           string           `json:"next_step,omitempty"`
	CompletedBy        string           `json:"completed_by,omitempty"`
	CompletedAt        string           `json:"completed_at,omitempty"`
	CreatedAt          string           `json:"created_at"`
	UpdatedAt          string           `json:"updated_at"`
	Revision           int64            `json:"revision"`
	LastEvent          SessionEvent     `json:"-"`
}

type SessionSpec struct {
	Title       string `json:"title"`
	Description string `json:"description"`
	Strategy    string `json:"strategy"`
}

type SessionSpecRevision struct {
	Key            string      `json:"key"`
	SessionID      string      `json:"session_id"`
	SourceRevision int64       `json:"source_revision"`
	Actor          string      `json:"actor"`
	Reason         string      `json:"reason"`
	At             string      `json:"at"`
	Before         SessionSpec `json:"before"`
	After          SessionSpec `json:"after"`
}

type SessionCheckpointRecord struct {
	Key       string `json:"key"`
	SessionID string `json:"session_id"`
	Sequence  int64  `json:"sequence"`
	Revision  int64  `json:"revision"`
	Actor     string `json:"actor"`
	At        string `json:"at"`
	Summary   string `json:"summary"`
	Problems  string `json:"problems,omitempty"`
	Decisions string `json:"decisions,omitempty"`
	Strategy  string `json:"strategy,omitempty"`
	NextStep  string `json:"next_step"`
}

type SessionEvent struct {
	Key          string                   `json:"key"`
	SessionID    string                   `json:"session_id"`
	Revision     int64                    `json:"revision"`
	Type         string                   `json:"type"`
	Actor        string                   `json:"actor"`
	At           string                   `json:"at"`
	FromStatus   Status                   `json:"from_status,omitempty"`
	ToStatus     Status                   `json:"to_status"`
	Summary      string                   `json:"summary"`
	NextStep     string                   `json:"next_step,omitempty"`
	Checkpoint   *SessionCheckpointRecord `json:"checkpoint,omitempty"`
	SpecRevision *SessionSpecRevision     `json:"spec_revision,omitempty"`
}

type SessionDetail struct {
	Session       Session                   `json:"session"`
	Events        []SessionEvent            `json:"events"`
	Checkpoints   []SessionCheckpointRecord `json:"checkpoints"`
	SpecRevisions []SessionSpecRevision     `json:"spec_revisions"`
	Tasks         []CatalogItem             `json:"tasks"`
}

type SessionSummary struct {
	ID              string `json:"id"`
	Title           string `json:"title"`
	Status          Status `json:"status"`
	Owner           string `json:"owner,omitempty"`
	ProgressSummary string `json:"progress_summary,omitempty"`
	NextStep        string `json:"next_step,omitempty"`
	UpdatedAt       string `json:"updated_at"`
	Revision        int64  `json:"revision"`
}

type SessionSearchResult struct {
	SessionSummary
	Score float64 `json:"score,omitempty"`
}

type SessionCreateInput struct{ Title, Description, Strategy, IdempotencyKey, Actor string }
type SessionListOptions struct {
	Status, Owner string
	Active        bool
}
type SessionReviseInput struct {
	ExpectedRevision             int64
	Reason                       string
	Title, Description, Strategy *string
}
type SessionCheckpointInput struct {
	Summary   string `json:"summary"`
	Problems  string `json:"problems,omitempty"`
	Decisions string `json:"decisions,omitempty"`
	Strategy  string `json:"strategy,omitempty"`
	NextStep  string `json:"next_step"`
}
