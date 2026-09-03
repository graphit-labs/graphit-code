package task

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"time"
)

const MaxBatchOperations = 100

type BatchInput struct {
	Operations []BatchOperation `json:"operations"`
	Lease      string           `json:"lease,omitempty"`
	Actor      string           `json:"-"`
}

type BatchOperation struct {
	Key                string   `json:"key,omitempty" jsonschema:"Optional caller correlation key"`
	Action             string   `json:"action" jsonschema:"create, claim, progress, heartbeat, release, complete, cancel, remove, flag, unflag, check, comment, dependency_add, or dependency_remove"`
	ID                 string   `json:"id,omitempty" jsonschema:"Task ID for every action except create"`
	ClaimToken         string   `json:"claim_token,omitempty" jsonschema:"Fencing token for owner mutations"`
	Lease              string   `json:"lease,omitempty" jsonschema:"Per-item lease override such as 2h"`
	Title              string   `json:"title,omitempty" jsonschema:"Create: concise task title"`
	Description        string   `json:"description,omitempty" jsonschema:"Create: robust self-contained specification"`
	AcceptanceCriteria []string `json:"acceptance_criteria,omitempty" jsonschema:"Create: observable acceptance criteria"`
	Tests              []string `json:"tests,omitempty" jsonschema:"Create: concrete tests or validations"`
	Type               string   `json:"type,omitempty" jsonschema:"Create: task type"`
	Priority           *int     `json:"priority,omitempty" jsonschema:"Create: priority 0 through 4; default 2"`
	ParentID           string   `json:"parent_id,omitempty" jsonschema:"Create: parent task ID"`
	DependsOn          []string `json:"depends_on,omitempty" jsonschema:"Create: blocking task IDs"`
	IdempotencyKey     string   `json:"idempotency_key,omitempty" jsonschema:"Create or comment: stable caller key"`
	Summary            string   `json:"summary,omitempty" jsonschema:"Progress, release, or complete summary"`
	NextStep           string   `json:"next_step,omitempty" jsonschema:"Progress or release continuation step"`
	Reason             string   `json:"reason,omitempty" jsonschema:"Cancel, remove, or flag reason"`
	ConfirmID          string   `json:"confirm_id,omitempty" jsonschema:"Remove: exact task ID confirmation"`
	CheckID            string   `json:"check_id,omitempty" jsonschema:"Check: acceptance or test check ID"`
	Passed             *bool    `json:"passed,omitempty" jsonschema:"Check: whether the check passed"`
	Evidence           string   `json:"evidence,omitempty" jsonschema:"Check: concrete verification evidence"`
	Kind               string   `json:"kind,omitempty" jsonschema:"Comment: note, decision, problem, lesson, or knowledge"`
	Body               string   `json:"body,omitempty" jsonschema:"Comment: durable self-contained body"`
	DependencyID       string   `json:"dependency_id,omitempty" jsonschema:"Dependency actions: blocking task ID"`
}

type BatchItemResult struct {
	Index  int    `json:"index"`
	Key    string `json:"key,omitempty"`
	Action string `json:"action"`
	ID     string `json:"id,omitempty"`
	OK     bool   `json:"ok"`
	Value  any    `json:"value,omitempty"`
	Error  string `json:"error,omitempty"`
}

type BatchResult struct {
	Results   []BatchItemResult `json:"results"`
	Succeeded int               `json:"succeeded"`
	Failed    int               `json:"failed"`
}

func (s *Service) Batch(ctx context.Context, in BatchInput) (BatchResult, error) {
	if len(in.Operations) == 0 {
		return BatchResult{}, errors.New("at least one batch operation is required")
	}
	if len(in.Operations) > MaxBatchOperations {
		return BatchResult{}, fmt.Errorf("batch has %d operations; maximum is %d", len(in.Operations), MaxBatchOperations)
	}
	in.Actor = strings.TrimSpace(in.Actor)
	if in.Actor == "" {
		return BatchResult{}, errors.New("agent id is required")
	}
	defaultLease, err := batchLease(in.Lease, DefaultLease)
	if err != nil {
		return BatchResult{}, err
	}

	out := BatchResult{Results: make([]BatchItemResult, 0, len(in.Operations))}
	for index, operation := range in.Operations {
		item := BatchItemResult{Index: index, Key: operation.Key, Action: strings.ToLower(strings.TrimSpace(operation.Action)), ID: strings.TrimSpace(operation.ID)}
		value, runErr := s.runBatchOperation(ctx, in.Actor, defaultLease, operation)
		if runErr != nil {
			item.Error = runErr.Error()
			out.Failed++
		} else {
			item.OK = true
			item.Value = value
			out.Succeeded++
			if item.ID == "" {
				switch created := value.(type) {
				case Task:
					item.ID = created.ID
				case Comment:
					item.ID = created.TaskID
				case Removal:
					item.ID = created.ID
				}
			}
		}
		out.Results = append(out.Results, item)
	}
	return out, nil
}

func (s *Service) runBatchOperation(ctx context.Context, actor string, defaultLease time.Duration, operation BatchOperation) (any, error) {
	action := strings.ToLower(strings.TrimSpace(operation.Action))
	lease, err := batchLease(operation.Lease, defaultLease)
	if err != nil {
		return nil, err
	}
	priority := 2
	if operation.Priority != nil {
		priority = *operation.Priority
	}
	switch action {
	case "create":
		return s.Create(ctx, CreateInput{Title: operation.Title, Description: operation.Description, AcceptanceCriteria: operation.AcceptanceCriteria, Tests: operation.Tests, Type: operation.Type, Priority: priority, ParentID: operation.ParentID, DependsOn: operation.DependsOn, IdempotencyKey: operation.IdempotencyKey, Actor: actor})
	case "claim":
		return s.Claim(ctx, operation.ID, actor, lease)
	case "progress":
		return s.Progress(ctx, operation.ID, operation.ClaimToken, actor, operation.Summary, operation.NextStep, lease)
	case "heartbeat":
		return s.Heartbeat(ctx, operation.ID, operation.ClaimToken, actor, lease)
	case "release":
		return s.Release(ctx, operation.ID, operation.ClaimToken, actor, operation.Summary, operation.NextStep)
	case "complete":
		return s.Complete(ctx, operation.ID, operation.ClaimToken, actor, operation.Summary)
	case "cancel":
		return s.Cancel(ctx, operation.ID, operation.ClaimToken, actor, operation.Reason)
	case "remove":
		return s.Remove(ctx, operation.ID, operation.ConfirmID, actor, operation.Reason)
	case "flag":
		return s.Flag(ctx, operation.ID, operation.ClaimToken, actor, operation.Reason)
	case "unflag":
		return s.Unflag(ctx, operation.ID, operation.ClaimToken, actor)
	case "check":
		if operation.Passed == nil {
			return nil, errors.New("check action requires passed")
		}
		return s.VerifyCheck(ctx, operation.ID, operation.ClaimToken, actor, operation.CheckID, *operation.Passed, operation.Evidence, lease)
	case "comment":
		return s.AddComment(ctx, operation.ID, operation.ClaimToken, actor, operation.Kind, operation.Body, operation.IdempotencyKey, lease)
	case "dependency_add":
		return s.AddDependency(ctx, operation.ID, operation.DependencyID, actor)
	case "dependency_remove":
		return s.RemoveDependency(ctx, operation.ID, operation.DependencyID, actor)
	default:
		return nil, fmt.Errorf("unsupported task batch action %q", operation.Action)
	}
}

func batchLease(value string, fallback time.Duration) (time.Duration, error) {
	if strings.TrimSpace(value) == "" {
		return fallback, nil
	}
	lease, err := time.ParseDuration(value)
	if err != nil || lease <= 0 {
		return 0, fmt.Errorf("invalid positive lease duration %q", value)
	}
	return lease, nil
}
