package task

import "time"

type Status string

const (
	StatusOpen       Status = "open"
	StatusInProgress Status = "in_progress"
	StatusCompleted  Status = "completed"
	StatusCancelled  Status = "cancelled"
)

func ValidStatus(v string) bool {
	switch Status(v) {
	case StatusOpen, StatusInProgress, StatusCompleted, StatusCancelled:
		return true
	}
	return false
}

// Task is the authoritative current snapshot. DependsOn and LastEvent are
// duplicated into query tables, but stay here so a single-row CAS contains
// everything needed to decide whether the task may be claimed or resumed.
type Task struct {
	ID             string   `json:"id"`
	ProjectID      string   `json:"project_id"`
	ParentID       string   `json:"parent_id,omitempty"`
	IdempotencyKey string   `json:"idempotency_key"`
	Title          string   `json:"title"`
	Description    string   `json:"description"`
	Type           string   `json:"type"`
	Status         Status   `json:"status"`
	Priority       int      `json:"priority"`
	DependsOn      []string `json:"depends_on,omitempty"`
	Checks         []Check  `json:"checks"`
	Flagged        bool     `json:"flagged"`
	FlagReason     string   `json:"flag_reason,omitempty"`

	Owner          string `json:"owner,omitempty"`
	ClaimToken     string `json:"claim_token,omitempty"`
	ClaimEpoch     int64  `json:"claim_epoch"`
	ClaimedAt      string `json:"claimed_at,omitempty"`
	LeaseExpiresAt string `json:"lease_expires_at,omitempty"`
	HeartbeatAt    string `json:"heartbeat_at,omitempty"`

	ProgressSequence int64  `json:"progress_sequence"`
	CommentSequence  int64  `json:"comment_sequence"`
	ProgressSummary  string `json:"progress_summary,omitempty"`
	NextStep         string `json:"next_step,omitempty"`
	CompletedBy      string `json:"completed_by,omitempty"`
	CompletedAt      string `json:"completed_at,omitempty"`

	CreatedAt string `json:"created_at"`
	UpdatedAt string `json:"updated_at"`
	Revision  int64  `json:"revision"`

	LastEvent   Event    `json:"-"`
	LastComment Comment  `json:"-"`
	Ready       bool     `json:"ready"`
	BlockedBy   []string `json:"blocked_by,omitempty"`
}

type Check struct {
	ID                 string `json:"id"`
	Kind               string `json:"kind"`
	Text               string `json:"text"`
	Status             string `json:"status"`
	Evidence           string `json:"evidence,omitempty"`
	VerifiedBy         string `json:"verified_by,omitempty"`
	VerifiedAt         string `json:"verified_at,omitempty"`
	SupersededBy       string `json:"superseded_by,omitempty"`
	SupersededReason   string `json:"superseded_reason,omitempty"`
	SupersededAt       string `json:"superseded_at,omitempty"`
	ReplacementCheckID string `json:"replacement_check_id,omitempty"`
}

type Comment struct {
	ID             string `json:"id"`
	TaskID         string `json:"task_id"`
	IdempotencyKey string `json:"idempotency_key"`
	Sequence       int64  `json:"sequence"`
	Kind           string `json:"kind"`
	Body           string `json:"body"`
	Actor          string `json:"actor"`
	At             string `json:"at"`
	Revision       int64  `json:"revision"`
}

type Event struct {
	Key          string        `json:"key"`
	TaskID       string        `json:"task_id"`
	Sequence     int64         `json:"sequence"`
	Type         string        `json:"type"`
	Actor        string        `json:"actor"`
	At           string        `json:"at"`
	FromStatus   Status        `json:"from_status,omitempty"`
	ToStatus     Status        `json:"to_status,omitempty"`
	Summary      string        `json:"summary,omitempty"`
	NextStep     string        `json:"next_step,omitempty"`
	Revision     int64         `json:"revision"`
	SpecRevision *SpecRevision `json:"spec_revision,omitempty"`
}

type Detail struct {
	Task          Task           `json:"task"`
	Events        []Event        `json:"events"`
	Comments      []Comment      `json:"comments"`
	SpecRevisions []SpecRevision `json:"spec_revisions"`
}

type TaskSpec struct {
	Title       string   `json:"title"`
	Description string   `json:"description"`
	Type        string   `json:"type"`
	Priority    int      `json:"priority"`
	ParentID    string   `json:"parent_id,omitempty"`
	DependsOn   []string `json:"depends_on,omitempty"`
	Checks      []Check  `json:"checks"`
}

type SpecRevision struct {
	Key            string   `json:"key"`
	TaskID         string   `json:"task_id"`
	SourceRevision int64    `json:"source_revision"`
	Kind           string   `json:"kind"`
	SubjectID      string   `json:"subject_id,omitempty"`
	Actor          string   `json:"actor"`
	Reason         string   `json:"reason"`
	At             string   `json:"at"`
	Before         TaskSpec `json:"before"`
	After          TaskSpec `json:"after"`
}

type CreateInput struct {
	Title              string
	Description        string
	AcceptanceCriteria []string
	Tests              []string
	Type               string
	Priority           int
	ParentID           string
	DependsOn          []string
	IdempotencyKey     string
	Actor              string
}

type ReviseInput struct {
	ExpectedRevision      int64
	Reason                string
	Title                 *string
	Description           *string
	Type                  *string
	Priority              *int
	ParentID              *string
	DependsOn             *[]string
	AddAcceptanceCriteria []string
	AddTests              []string
}

type SupersedeCheckInput struct {
	ExpectedRevision int64
	CheckID          string
	Reason           string
	ReplacementText  string
	ReplacementKind  string
}

type ListOptions struct {
	Status   string
	Owner    string
	ParentID string
	Ready    bool
}

type SearchResult struct {
	ID       string  `json:"id"`
	Title    string  `json:"title"`
	Status   Status  `json:"status"`
	Priority int     `json:"priority"`
	Ready    bool    `json:"ready"`
	Flagged  bool    `json:"flagged"`
	Score    float64 `json:"score,omitempty"`
}

type Removal struct {
	ID        string `json:"id"`
	Reason    string `json:"reason"`
	RemovedBy string `json:"removed_by"`
	RemovedAt string `json:"removed_at"`
}

const DefaultLease = time.Hour
