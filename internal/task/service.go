package task

import (
	"context"
	"crypto/sha256"
	"errors"
	"fmt"
	"reflect"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/oklog/ulid/v2"
)

var (
	ErrNotFound   = errors.New("task not found")
	ErrClaimed    = errors.New("task is claimed by another agent")
	ErrBlocked    = errors.New("task has incomplete dependencies")
	ErrFence      = errors.New("task claim token is stale or invalid")
	ErrConcurrent = errors.New("task changed concurrently")
	ErrReferenced = errors.New("task is referenced by another task")
	ErrDisabled   = errors.New("task module is disabled for this project")
)

type Service struct {
	projectID, uri   string
	s3               config.S3Config
	now              func() time.Time
	operationTimeout time.Duration
	versionRetention time.Duration
}

const (
	defaultOperationTimeout = 30 * time.Second
	maintenanceTimeout      = 5 * time.Minute
	defaultVersionRetention = 15 * time.Minute
	schedulerLeaseDuration  = 30 * time.Second
	schedulerReleaseTimeout = 5 * time.Second
)

func Open(projectDir string) (*Service, error) {
	id := store.ProjectID(projectDir)
	if id == "" {
		return nil, fmt.Errorf("project is not initialized")
	}
	projectCfg := config.LoadProjectConfig(projectDir)
	if config.IsModuleDisabled("task", nil, projectCfg) {
		return nil, ErrDisabled
	}
	return &Service{
		projectID: id, uri: TableURI(id, projectCfg), s3: config.ResolveHubS3(nil, projectCfg), now: time.Now,
		operationTimeout: config.ResolveTaskOperationTimeout(nil, projectCfg),
		versionRetention: config.ResolveTaskVersionRetention(nil, projectCfg),
	}, nil
}

func OpenAt(projectID, uri string) *Service {
	return &Service{
		projectID: projectID, uri: uri, now: time.Now,
		operationTimeout: defaultOperationTimeout, versionRetention: defaultVersionRetention,
	}
}

func (s *Service) withTables(ctx context.Context, fn func(*tables) error) error {
	ctx, cancel := boundedContext(ctx, s.operationTimeout)
	defer cancel()
	t, err := openTables(ctx, s.uri, s.s3)
	if err != nil {
		return taskStorageError(ctx, err)
	}
	defer t.close()
	return taskStorageError(ctx, fn(t))
}

func (s *Service) withLock(ctx context.Context, actor string, fn func(*tables) error) error {
	return s.withLockReconcile(ctx, actor, false, fn)
}

func (s *Service) withLockFast(ctx context.Context, actor string, fn func(*tables) error) error {
	return s.withLockReconcile(ctx, actor, false, fn)
}

func (s *Service) withLockReconcile(ctx context.Context, actor string, reconcile bool, fn func(*tables) error) error {
	return s.withLockTimeout(ctx, actor, reconcile, s.operationTimeout, fn)
}

func (s *Service) withLockTimeout(ctx context.Context, actor string, reconcile bool, timeout time.Duration, fn func(*tables) error) error {
	ctx, cancel := boundedContext(ctx, timeout)
	defer cancel()
	if strings.TrimSpace(actor) == "" {
		actor = "system"
	}
	t, err := openTables(ctx, s.uri, s.s3)
	if err != nil {
		return taskStorageError(ctx, err)
	}
	defer t.close()
	token := ulid.Make().String()
	leaseDuration := schedulerLeaseDuration
	if timeout > leaseDuration {
		leaseDuration = timeout
	}
	for attempt := 0; attempt < 8; attempt++ {
		now := s.now().UTC()
		row := lancestore.Row{"key": "scheduler", "token": token, "owner": actor, "acquired_at": stamp(now), "expires_at": stamp(now.Add(leaseDuration)), "revision": int64(1)}
		res, merr := t.control.Merge(ctx, lancestore.MergeOptions{KeyColumn: "key", MatchCondition: "target.token = '' OR target.expires_at <= source.acquired_at", InsertIfMissing: true}, []lancestore.Row{row})
		if merr != nil {
			return taskStorageError(ctx, fmt.Errorf("acquiring task scheduler lease: %w", merr))
		}
		if res.Changed() {
			defer func() {
				releaseCtx, releaseCancel := context.WithTimeout(context.Background(), schedulerReleaseTimeout)
				defer releaseCancel()
				release := lancestore.Row{"key": "scheduler", "token": "", "owner": "", "acquired_at": stamp(s.now().UTC()), "expires_at": "", "revision": int64(1)}
				_, _ = t.control.Merge(releaseCtx, lancestore.MergeOptions{KeyColumn: "key", MatchCondition: "target.token = " + quote(token)}, []lancestore.Row{release})
			}()
			if err := t.refresh(ctx); err != nil {
				return taskStorageError(ctx, fmt.Errorf("refreshing task tables after scheduler lease: %w", err))
			}
			if reconcile {
				if err := s.reconcileLocked(ctx, t, actor); err != nil {
					return err
				}
			}
			return taskStorageError(ctx, fn(t))
		}
		select {
		case <-ctx.Done():
			return taskStorageError(ctx, ctx.Err())
		case <-time.After(time.Duration(attempt+1) * 25 * time.Millisecond):
		}
	}
	return fmt.Errorf("task scheduler is busy; retry the operation")
}

func (s *Service) Maintain(ctx context.Context) error {
	_, err := s.maintain(ctx)
	return err
}

func (s *Service) maintain(ctx context.Context) (maintenanceResult, error) {
	retention := s.versionRetention
	if retention <= 0 {
		retention = defaultVersionRetention
	}
	var result maintenanceResult
	err := s.withLockTimeout(ctx, "maintenance", false, maintenanceTimeout, func(t *tables) error {
		if err := t.ensureIndexes(ctx); err != nil {
			return err
		}
		var err error
		result, err = t.maintain(ctx, retention)
		return err
	})
	return result, err
}

func boundedContext(ctx context.Context, timeout time.Duration) (context.Context, context.CancelFunc) {
	if timeout <= 0 {
		timeout = defaultOperationTimeout
	}
	if deadline, ok := ctx.Deadline(); ok && time.Until(deadline) <= timeout {
		return ctx, func() {}
	}
	return context.WithTimeout(ctx, timeout)
}

func taskStorageError(ctx context.Context, err error) error {
	if err == nil {
		return nil
	}
	if ctxErr := ctx.Err(); ctxErr != nil {
		return fmt.Errorf("task storage operation: %w", ctxErr)
	}
	return err
}

func (s *Service) Create(ctx context.Context, in CreateInput) (Task, error) {
	in.Title = strings.TrimSpace(in.Title)
	if in.Title == "" {
		return Task{}, errors.New("task title is required")
	}
	in.Description = strings.TrimSpace(in.Description)
	if in.Description == "" {
		return Task{}, errors.New("task description is required and must contain the self-contained specification")
	}
	if len(nonEmpty(in.AcceptanceCriteria)) == 0 {
		return Task{}, errors.New("at least one acceptance criterion is required")
	}
	if len(nonEmpty(in.Tests)) == 0 {
		return Task{}, errors.New("at least one test or validation is required")
	}
	if in.Type == "" {
		in.Type = "task"
	}
	if in.Priority < 0 || in.Priority > 4 {
		return Task{}, errors.New("priority must be between 0 and 4")
	}
	key := strings.TrimSpace(in.IdempotencyKey)
	if key == "" {
		key = canonicalKey(in.Title)
	}
	var out Task
	err := s.withLock(ctx, in.Actor, func(t *tables) error {
		if err := t.ensureIndexes(ctx); err != nil {
			return err
		}
		if current, ok, err := t.getTaskByIdempotencyKey(ctx, key); err != nil {
			return err
		} else if ok {
			out = current
			return nil
		}
		id, err := s.availableTaskID(ctx, t, key)
		if err != nil {
			return err
		}
		checks, err := buildChecks(id, in.AcceptanceCriteria, in.Tests)
		if err != nil {
			return err
		}
		deps := uniqueSorted(in.DependsOn)
		if err := s.validateDependencies(ctx, t, id, deps); err != nil {
			return err
		}
		parentID := strings.TrimSpace(in.ParentID)
		if parentID != "" {
			if parentID == id {
				return errors.New("a task cannot be its own parent")
			}
			if pending, err := removalPending(ctx, t, parentID); err != nil {
				return err
			} else if pending {
				return fmt.Errorf("parent task %s is pending removal", parentID)
			}
			parent, ok, err := t.getTask(ctx, parentID)
			if err != nil {
				return err
			}
			if !ok {
				return fmt.Errorf("parent %s: %w", parentID, ErrNotFound)
			}
			if parent.Status == StatusCompleted || parent.Status == StatusCancelled {
				return fmt.Errorf("parent task %s is already %s", parentID, parent.Status)
			}
		}
		now := stamp(s.now().UTC())
		out = Task{ID: id, ProjectID: s.projectID, ParentID: parentID, IdempotencyKey: key, Title: in.Title, Description: in.Description, Type: in.Type, Status: StatusOpen, Priority: in.Priority, DependsOn: deps, Checks: checks, CreatedAt: now, UpdatedAt: now, Revision: 1}
		out.LastEvent = newEvent(out, "created", in.Actor, "", StatusOpen, "task created", out.NextStep)
		revision := newSpecRevision(Task{}, out, "created", in.Actor, "task created")
		out.LastEvent.SpecRevision = &revision
		res, err := t.tasks.Merge(ctx, lancestore.MergeOptions{KeyColumn: "id", MatchCondition: "false", InsertIfMissing: true}, []lancestore.Row{taskRow(out)})
		if err != nil {
			return err
		}
		if !res.Changed() {
			out, _, err = t.getTask(ctx, id)
			return err
		}
		return s.projectTask(ctx, t, out, in.Actor)
	})
	return out, err
}

func (s *Service) Revise(ctx context.Context, id, token, actor string, in ReviseInput, lease time.Duration) (Task, error) {
	actor = strings.TrimSpace(actor)
	in.Reason = strings.TrimSpace(in.Reason)
	if actor == "" {
		return Task{}, errors.New("agent id is required")
	}
	if in.ExpectedRevision < 1 {
		return Task{}, errors.New("expected revision is required")
	}
	if in.Reason == "" {
		return Task{}, errors.New("revision reason is required")
	}
	var out Task
	err := s.withLock(ctx, actor, func(t *tables) error {
		current, ok, err := t.getTask(ctx, id)
		if err != nil {
			return err
		}
		if !ok {
			return ErrNotFound
		}
		if current.Status != StatusInProgress || token == "" || current.ClaimToken != token || (actor != "" && current.Owner != actor) {
			return ErrFence
		}
		if current.Revision != in.ExpectedRevision {
			return fmt.Errorf("%w: expected revision %d, current revision is %d", ErrConcurrent, in.ExpectedRevision, current.Revision)
		}
		next := current
		if in.Title != nil {
			next.Title = strings.TrimSpace(*in.Title)
			if next.Title == "" {
				return errors.New("task title is required")
			}
		}
		if in.Description != nil {
			next.Description = strings.TrimSpace(*in.Description)
			if next.Description == "" {
				return errors.New("task description is required and must contain the self-contained specification")
			}
		}
		if in.Type != nil {
			next.Type = strings.TrimSpace(*in.Type)
			if next.Type == "" {
				return errors.New("task type is required")
			}
		}
		if in.Priority != nil {
			if *in.Priority < 0 || *in.Priority > 4 {
				return errors.New("priority must be between 0 and 4")
			}
			next.Priority = *in.Priority
		}
		all, err := t.allTasks(ctx)
		if err != nil {
			return err
		}
		if in.ParentID != nil {
			next.ParentID = strings.TrimSpace(*in.ParentID)
			if next.ParentID != "" {
				if pending, err := removalPending(ctx, t, next.ParentID); err != nil {
					return err
				} else if pending {
					return fmt.Errorf("parent task %s is pending removal", next.ParentID)
				}
			}
			if err := validateParentChange(next.ID, next.ParentID, all); err != nil {
				return err
			}
		}
		if in.DependsOn != nil {
			next.DependsOn = uniqueSorted(*in.DependsOn)
			if err := s.validateDependencies(ctx, t, id, next.DependsOn); err != nil {
				return err
			}
		}
		if len(nonEmpty(in.AddAcceptanceCriteria))+len(nonEmpty(in.AddTests)) > 0 {
			added, err := buildChecks(id, in.AddAcceptanceCriteria, in.AddTests)
			if err != nil {
				return err
			}
			seen := make(map[string]bool, len(next.Checks))
			for _, check := range next.Checks {
				seen[check.ID] = true
			}
			for _, check := range added {
				if !seen[check.ID] {
					next.Checks = append(next.Checks, check)
					seen[check.ID] = true
				}
			}
			sortChecks(next.Checks)
		}
		if next.Title != current.Title || next.Description != current.Description || next.Type != current.Type {
			for i := range next.Checks {
				if next.Checks[i].Status == "superseded" {
					continue
				}
				next.Checks[i].Status = "pending"
				next.Checks[i].Evidence = ""
				next.Checks[i].VerifiedBy = ""
				next.Checks[i].VerifiedAt = ""
			}
		}
		beforeSpec, afterSpec := taskSpec(current), taskSpec(next)
		if reflect.DeepEqual(beforeSpec, afterSpec) {
			return errors.New("task revision has no specification changes")
		}
		now := s.now().UTC()
		if lease <= 0 {
			lease = DefaultLease
		}
		next.HeartbeatAt = stamp(now)
		next.LeaseExpiresAt = renewedLeaseExpiry(current.LeaseExpiresAt, now, lease)
		next.Revision++
		next.UpdatedAt = stamp(now)
		next.LastEvent = newEvent(next, "revised", actor, current.Status, next.Status, in.Reason, next.NextStep)
		revision := newSpecRevision(current, next, "revised", actor, in.Reason)
		next.LastEvent.SpecRevision = &revision
		if err := s.putCAS(ctx, t, current.Revision, next); err != nil {
			return err
		}
		out = next
		return s.projectTask(ctx, t, next, actor)
	})
	return out, err
}

func (s *Service) SupersedeCheck(ctx context.Context, id, token, actor string, in SupersedeCheckInput, lease time.Duration) (Task, error) {
	actor = strings.TrimSpace(actor)
	in.CheckID = strings.TrimSpace(in.CheckID)
	in.Reason = strings.TrimSpace(in.Reason)
	in.ReplacementText = strings.TrimSpace(in.ReplacementText)
	in.ReplacementKind = strings.ToLower(strings.TrimSpace(in.ReplacementKind))
	if actor == "" {
		return Task{}, errors.New("agent id is required")
	}
	if in.ExpectedRevision < 1 {
		return Task{}, errors.New("expected revision is required")
	}
	if in.CheckID == "" || in.Reason == "" {
		return Task{}, errors.New("check id and supersession reason are required")
	}
	var out Task
	err := s.withLock(ctx, actor, func(t *tables) error {
		current, ok, err := t.getTask(ctx, id)
		if err != nil {
			return err
		}
		if !ok {
			return ErrNotFound
		}
		if current.Status != StatusInProgress || token == "" || current.ClaimToken != token || (actor != "" && current.Owner != actor) {
			return ErrFence
		}
		if current.Revision != in.ExpectedRevision {
			return fmt.Errorf("%w: expected revision %d, current revision is %d", ErrConcurrent, in.ExpectedRevision, current.Revision)
		}
		next := current
		index := -1
		for i := range next.Checks {
			if next.Checks[i].ID == in.CheckID {
				index = i
				break
			}
		}
		if index < 0 {
			return fmt.Errorf("check %s: %w", in.CheckID, ErrNotFound)
		}
		if next.Checks[index].Status == "superseded" {
			return fmt.Errorf("check %s is already superseded", in.CheckID)
		}
		now := s.now().UTC()
		next.Checks[index].Status = "superseded"
		next.Checks[index].SupersededBy = actor
		next.Checks[index].SupersededReason = in.Reason
		next.Checks[index].SupersededAt = stamp(now)
		if in.ReplacementText != "" {
			kind := in.ReplacementKind
			if kind == "" {
				kind = next.Checks[index].Kind
			}
			if kind != "acceptance" && kind != "test" {
				return errors.New("replacement kind must be acceptance or test")
			}
			replacement := Check{ID: deterministicCheckID(id, kind, in.ReplacementText), Kind: kind, Text: in.ReplacementText, Status: "pending"}
			for _, check := range next.Checks {
				if check.ID == replacement.ID {
					return fmt.Errorf("replacement check %s already exists", replacement.ID)
				}
			}
			next.Checks[index].ReplacementCheckID = replacement.ID
			next.Checks = append(next.Checks, replacement)
			sortChecks(next.Checks)
		} else if in.ReplacementKind != "" {
			return errors.New("replacement kind requires replacement text")
		}
		if err := validateActiveCheckKinds(next.Checks); err != nil {
			return err
		}
		if lease <= 0 {
			lease = DefaultLease
		}
		next.HeartbeatAt = stamp(now)
		next.LeaseExpiresAt = renewedLeaseExpiry(current.LeaseExpiresAt, now, lease)
		next.Revision++
		next.UpdatedAt = stamp(now)
		next.LastEvent = newEvent(next, "check_superseded", actor, current.Status, next.Status, in.CheckID+": "+in.Reason, next.NextStep)
		revision := newSpecRevision(current, next, "check_superseded", actor, in.Reason)
		revision.SubjectID = in.CheckID
		next.LastEvent.SpecRevision = &revision
		if err := s.putCAS(ctx, t, current.Revision, next); err != nil {
			return err
		}
		out = next
		return s.projectTask(ctx, t, next, actor)
	})
	return out, err
}

const taskIDMinHashLength = 4

func (s *Service) availableTaskID(ctx context.Context, t *tables, key string) (string, error) {
	digest := taskIDDigest(s.projectID, key)
	for length := taskIDMinHashLength; length <= len(digest); length++ {
		id := "tsk-" + digest[:length]
		current, exists, err := t.getTask(ctx, id)
		if err != nil {
			return "", err
		}
		if !exists || current.IdempotencyKey == key {
			return id, nil
		}
	}
	return "", fmt.Errorf("cannot allocate a collision-free task id")
}

func (s *Service) Claim(ctx context.Context, id, actor string, lease time.Duration) (Task, error) {
	if strings.TrimSpace(actor) == "" {
		return Task{}, errors.New("agent id is required")
	}
	if lease <= 0 {
		lease = DefaultLease
	}
	var out Task
	err := s.withLock(ctx, actor, func(t *tables) error {
		all, err := t.allTasks(ctx)
		if err != nil {
			return err
		}
		byID := indexTasks(all)
		current, ok := byID[id]
		if !ok {
			return ErrNotFound
		}
		if current.Status == StatusInProgress && current.LeaseExpiresAt != "" && current.LeaseExpiresAt <= stamp(s.now().UTC()) {
			old := current
			current.Status = StatusOpen
			clearClaim(&current)
			current.Revision++
			current.UpdatedAt = stamp(s.now().UTC())
			current.LastEvent = newEvent(current, "lease_expired", "system", old.Status, current.Status, "claim expired; task is ready for takeover", current.NextStep)
			if err := s.putCAS(ctx, t, old.Revision, current); err != nil {
				return err
			}
			if err := s.projectTask(ctx, t, current, actor); err != nil {
				return err
			}
			byID[id] = current
		}
		if current.Status == StatusInProgress {
			return ErrClaimed
		}
		if current.Status != StatusOpen {
			return fmt.Errorf("task %s is %s", id, current.Status)
		}
		blocked := blockedBy(current, byID)
		if len(blocked) > 0 {
			return fmt.Errorf("%w: %s", ErrBlocked, strings.Join(blocked, ", "))
		}
		for _, other := range all {
			if other.Status == StatusInProgress && other.Owner == actor && other.LeaseExpiresAt > stamp(s.now().UTC()) {
				return fmt.Errorf("agent %q already owns %s", actor, other.ID)
			}
		}
		now := s.now().UTC()
		next := current
		next.Status = StatusInProgress
		next.Owner = actor
		next.ClaimToken = ulid.Make().String()
		next.ClaimEpoch++
		next.ClaimedAt = stamp(now)
		next.HeartbeatAt = stamp(now)
		next.LeaseExpiresAt = renewedLeaseExpiry(current.LeaseExpiresAt, now, lease)
		next.Revision++
		next.UpdatedAt = stamp(now)
		next.LastEvent = newEvent(next, "claimed", actor, current.Status, next.Status, "task claimed", next.NextStep)
		if err := s.putCAS(ctx, t, current.Revision, next); err != nil {
			return err
		}
		out = next
		return s.projectTask(ctx, t, next, actor)
	})
	return out, err
}

// ForceTakeover recovers an unexpired claim whose owner cannot release it.
// Exact confirmation and revision fencing keep this exceptional path explicit,
// while rotating the token prevents a recovered owner from writing later.
func (s *Service) ForceTakeover(ctx context.Context, id, actor string, in ForceTakeoverInput, lease time.Duration) (Task, error) {
	id = strings.TrimSpace(id)
	actor = strings.TrimSpace(actor)
	in.ConfirmID = strings.TrimSpace(in.ConfirmID)
	in.Reason = strings.TrimSpace(in.Reason)
	if id == "" || in.ConfirmID != id {
		return Task{}, errors.New("force takeover requires exact task id confirmation")
	}
	if actor == "" {
		return Task{}, errors.New("agent id is required")
	}
	if in.Reason == "" {
		return Task{}, errors.New("force takeover reason is required")
	}
	if in.ExpectedRevision <= 0 {
		return Task{}, errors.New("force takeover requires the current task revision")
	}
	if lease <= 0 {
		return Task{}, errors.New("force takeover requires a positive lease duration")
	}

	var out Task
	err := s.withLock(ctx, actor, func(t *tables) error {
		all, err := t.allTasks(ctx)
		if err != nil {
			return err
		}
		byID := indexTasks(all)
		current, ok := byID[id]
		if !ok {
			return ErrNotFound
		}
		if current.Revision != in.ExpectedRevision {
			return fmt.Errorf("%w: %s revision is %d, expected %d", ErrConcurrent, id, current.Revision, in.ExpectedRevision)
		}
		if current.Status != StatusInProgress {
			return fmt.Errorf("task %s is %s; force takeover requires in_progress", id, current.Status)
		}
		if current.Owner == actor {
			return errors.New("current owner must renew or release its claim instead of forcing takeover")
		}
		if current.Owner == "" || current.ClaimToken == "" || current.LeaseExpiresAt == "" {
			return fmt.Errorf("task %s has incomplete claim state", id)
		}
		now := s.now().UTC()
		if current.LeaseExpiresAt <= stamp(now) {
			return fmt.Errorf("task %s lease already expired; use normal claim", id)
		}
		for _, other := range all {
			if other.ID != id && other.Status == StatusInProgress && other.Owner == actor && other.LeaseExpiresAt > stamp(now) {
				return fmt.Errorf("agent %q already owns %s", actor, other.ID)
			}
		}

		next := current
		next.Owner = actor
		next.ClaimToken = ulid.Make().String()
		next.ClaimEpoch++
		next.ClaimedAt = stamp(now)
		next.HeartbeatAt = stamp(now)
		next.LeaseExpiresAt = stamp(now.Add(lease))
		next.Revision++
		next.UpdatedAt = stamp(now)
		next.LastEvent = newEvent(next, "force_takeover", actor, current.Status, next.Status,
			fmt.Sprintf("forced takeover from %q to %q at revision %d -> %d: %s", current.Owner, actor, current.Revision, next.Revision, in.Reason), next.NextStep)
		if err := s.putCAS(ctx, t, current.Revision, next); err != nil {
			return err
		}
		out = next
		return s.projectTask(ctx, t, next, actor)
	})
	return out, err
}

func (s *Service) Progress(ctx context.Context, id, token, actor, summary, nextStep string, lease time.Duration) (Task, error) {
	if strings.TrimSpace(summary) == "" {
		return Task{}, errors.New("progress summary is required")
	}
	return s.claimMutation(ctx, id, token, actor, lease, "progress", func(v *Task) error {
		v.ProgressSequence++
		v.ProgressSummary = strings.TrimSpace(summary)
		v.NextStep = strings.TrimSpace(nextStep)
		return nil
	})
}

func (s *Service) Heartbeat(ctx context.Context, id, token, actor string, lease time.Duration) (Task, error) {
	return s.claimMutation(ctx, id, token, actor, lease, "heartbeat", func(v *Task) error { return nil })
}

func (s *Service) Release(ctx context.Context, id, token, actor, summary, nextStep string) (Task, error) {
	return s.claimMutation(ctx, id, token, actor, 0, "released", func(v *Task) error {
		if strings.TrimSpace(summary) != "" {
			v.ProgressSequence++
			v.ProgressSummary = strings.TrimSpace(summary)
		}
		if strings.TrimSpace(nextStep) != "" {
			v.NextStep = strings.TrimSpace(nextStep)
		}
		v.Status = StatusOpen
		clearClaim(v)
		return nil
	})
}

func (s *Service) Complete(ctx context.Context, id, token, actor, summary string) (Task, error) {
	return s.claimMutation(ctx, id, token, actor, 0, "completed", func(v *Task) error {
		if strings.TrimSpace(summary) != "" {
			v.ProgressSequence++
			v.ProgressSummary = strings.TrimSpace(summary)
		}
		v.Status = StatusCompleted
		v.CompletedBy = actor
		v.CompletedAt = stamp(s.now().UTC())
		v.NextStep = ""
		clearClaim(v)
		return nil
	})
}

// Cancel records a durable terminal transition. Open work can be cancelled
// directly; in-progress work still requires its current fencing token so one
// agent cannot silently invalidate another agent's live claim.
func (s *Service) Cancel(ctx context.Context, id, token, actor, reason string) (Task, error) {
	actor = strings.TrimSpace(actor)
	reason = strings.TrimSpace(reason)
	if actor == "" {
		return Task{}, errors.New("agent id is required")
	}
	if reason == "" {
		return Task{}, errors.New("cancellation reason is required")
	}
	var out Task
	err := s.withLock(ctx, actor, func(t *tables) error {
		current, ok, err := t.getTask(ctx, id)
		if err != nil {
			return err
		}
		if !ok {
			return ErrNotFound
		}
		if current.Status == StatusCancelled {
			out = current
			return nil
		}
		if current.Status == StatusCompleted {
			return fmt.Errorf("completed task %s cannot be cancelled", id)
		}
		if current.Status == StatusInProgress && (token == "" || current.ClaimToken != token || current.Owner != actor) {
			return ErrFence
		}
		next := current
		next.Status = StatusCancelled
		next.Flagged = false
		next.FlagReason = ""
		next.ProgressSequence++
		next.ProgressSummary = "cancelled: " + reason
		next.NextStep = ""
		clearClaim(&next)
		next.Revision++
		next.UpdatedAt = stamp(s.now().UTC())
		next.LastEvent = newEvent(next, "cancelled", actor, current.Status, next.Status, reason, "")
		if err := s.putCAS(ctx, t, current.Revision, next); err != nil {
			return err
		}
		out = next
		return s.projectTask(ctx, t, next, actor)
	})
	return out, err
}

const taskRemovalPrefix = "remove/"

// Remove performs an explicitly confirmed hard cleanup. A durable intent row
// in task_control makes the cross-table delete resumable after interruption.
func (s *Service) Remove(ctx context.Context, id, confirmation, actor, reason string) (Removal, error) {
	id = strings.TrimSpace(id)
	confirmation = strings.TrimSpace(confirmation)
	actor = strings.TrimSpace(actor)
	reason = strings.TrimSpace(reason)
	if id == "" || confirmation != id {
		return Removal{}, errors.New("removal requires exact task id confirmation")
	}
	if actor == "" {
		return Removal{}, errors.New("agent id is required")
	}
	if reason == "" {
		return Removal{}, errors.New("removal reason is required")
	}
	var out Removal
	err := s.withLock(ctx, actor, func(t *tables) error {
		if _, ok, err := t.getTask(ctx, id); err != nil {
			return err
		} else if !ok {
			return ErrNotFound
		}
		all, err := t.allTasks(ctx)
		if err != nil {
			return err
		}
		if reference := taskReference(all, id); reference != "" {
			return fmt.Errorf("%w: %s", ErrReferenced, reference)
		}
		now := stamp(s.now().UTC())
		intent := lancestore.Row{"key": taskRemovalPrefix + id, "token": reason, "owner": actor, "acquired_at": now, "expires_at": "", "revision": int64(1)}
		if err := t.control.Upsert(ctx, "key", []lancestore.Row{intent}); err != nil {
			return err
		}
		if err := finishTaskRemoval(ctx, t, id); err != nil {
			return err
		}
		out = Removal{ID: id, Reason: reason, RemovedBy: actor, RemovedAt: now}
		return nil
	})
	return out, err
}

func finishTaskRemoval(ctx context.Context, t *tables, id string) error {
	if err := t.tasks.DeleteByKey(ctx, "id", []string{id}); err != nil {
		return err
	}
	filter := "task_id = " + quote(id)
	if err := t.events.DeleteWhere(ctx, filter); err != nil {
		return err
	}
	if err := t.comments.DeleteWhere(ctx, filter); err != nil {
		return err
	}
	if err := t.checks.DeleteWhere(ctx, filter); err != nil {
		return err
	}
	if err := t.specRevisions.DeleteWhere(ctx, filter); err != nil {
		return err
	}
	if err := t.dependencies.DeleteWhere(ctx, filter+" OR depends_on = "+quote(id)); err != nil {
		return err
	}
	return t.control.DeleteByKey(ctx, "key", []string{taskRemovalPrefix + id})
}

func (s *Service) Flag(ctx context.Context, id, token, actor, reason string) (Task, error) {
	if strings.TrimSpace(reason) == "" {
		return Task{}, errors.New("flag reason is required")
	}
	return s.claimMutation(ctx, id, token, actor, 0, "flagged", func(v *Task) error {
		v.Flagged = true
		v.FlagReason = strings.TrimSpace(reason)
		return nil
	})
}

func (s *Service) Unflag(ctx context.Context, id, token, actor string) (Task, error) {
	return s.claimMutation(ctx, id, token, actor, 0, "unflagged", func(v *Task) error {
		v.Flagged = false
		v.FlagReason = ""
		return nil
	})
}

func (s *Service) VerifyCheck(ctx context.Context, id, token, actor, checkID string, passed bool, evidence string, lease time.Duration) (Task, error) {
	evidence = strings.TrimSpace(evidence)
	if evidence == "" {
		return Task{}, errors.New("verification evidence is required")
	}
	eventType := "check_failed"
	if passed {
		eventType = "check_passed"
	}
	return s.claimMutation(ctx, id, token, actor, lease, eventType, func(v *Task) error {
		for i := range v.Checks {
			if v.Checks[i].ID != checkID {
				continue
			}
			if v.Checks[i].Status == "superseded" {
				return fmt.Errorf("check %s is superseded", checkID)
			}
			v.Checks[i].Status = "failed"
			if passed {
				v.Checks[i].Status = "passed"
			}
			v.Checks[i].Evidence = evidence
			v.Checks[i].VerifiedBy = actor
			v.Checks[i].VerifiedAt = stamp(s.now().UTC())
			return nil
		}
		return fmt.Errorf("check %s: %w", checkID, ErrNotFound)
	})
}

func (s *Service) AddComment(ctx context.Context, id, token, actor, kind, body, idempotencyKey string, lease time.Duration) (Comment, error) {
	kind = strings.ToLower(strings.TrimSpace(kind))
	if !validCommentKind(kind) {
		return Comment{}, errors.New("comment kind must be note, decision, problem, lesson, or knowledge")
	}
	body = strings.TrimSpace(body)
	if body == "" {
		return Comment{}, errors.New("comment body is required")
	}
	key := strings.TrimSpace(idempotencyKey)
	if key == "" {
		key = canonicalKey(kind + "-" + body)
	}
	commentID := deterministicCommentID(id, key)
	var out Comment
	err := s.withLock(ctx, actor, func(t *tables) error {
		hits, err := t.comments.Search(ctx, lancestore.Query{Filter: "id = " + quote(commentID), Limit: 1})
		if err != nil {
			return err
		}
		if len(hits) > 0 {
			out = commentFromRow(hits[0].Row)
			if out.IdempotencyKey != key {
				return errors.New("task comment id collision")
			}
			return nil
		}
		current, ok, err := t.getTask(ctx, id)
		if err != nil {
			return err
		}
		if !ok {
			return ErrNotFound
		}
		if current.Status != StatusInProgress || token == "" || current.ClaimToken != token || (actor != "" && current.Owner != actor) {
			return ErrFence
		}
		now := s.now().UTC()
		next := current
		next.CommentSequence++
		next.Revision++
		next.UpdatedAt = stamp(now)
		if lease <= 0 {
			lease = DefaultLease
		}
		next.HeartbeatAt = stamp(now)
		next.LeaseExpiresAt = renewedLeaseExpiry(current.LeaseExpiresAt, now, lease)
		out = Comment{ID: commentID, TaskID: id, IdempotencyKey: key, Sequence: next.CommentSequence, Kind: kind, Body: body, Actor: actor, At: next.UpdatedAt, Revision: next.Revision}
		next.LastComment = out
		next.LastEvent = newEvent(next, "commented", actor, current.Status, next.Status, kind+": "+body, next.NextStep)
		if err := s.putCAS(ctx, t, current.Revision, next); err != nil {
			return err
		}
		return s.projectTask(ctx, t, next, actor)
	})
	return out, err
}

func (s *Service) claimMutation(ctx context.Context, id, token, actor string, lease time.Duration, eventType string, apply func(*Task) error) (Task, error) {
	var out Task
	err := s.withLock(ctx, actor, func(t *tables) error {
		current, ok, err := t.getTask(ctx, id)
		if err != nil {
			return err
		}
		if !ok {
			return ErrNotFound
		}
		if current.Status != StatusInProgress || token == "" || current.ClaimToken != token {
			return ErrFence
		}
		if actor != "" && current.Owner != actor {
			return ErrFence
		}
		if eventType == "completed" && current.Flagged {
			return fmt.Errorf("task %s is flagged and cannot be completed: %s", id, current.FlagReason)
		}
		if eventType == "completed" {
			all, err := t.allTasks(ctx)
			if err != nil {
				return err
			}
			if violations := completionViolations(current, all); len(violations) > 0 {
				return fmt.Errorf("task %s cannot be completed: %s", id, strings.Join(violations, "; "))
			}
		}
		next := current
		if err := apply(&next); err != nil {
			return err
		}
		now := s.now().UTC()
		if next.Status == StatusInProgress {
			if lease <= 0 {
				lease = DefaultLease
			}
			next.HeartbeatAt = stamp(now)
			next.LeaseExpiresAt = renewedLeaseExpiry(current.LeaseExpiresAt, now, lease)
		}
		next.Revision++
		next.UpdatedAt = stamp(now)
		next.LastEvent = newEvent(next, eventType, actor, current.Status, next.Status, mutationSummary(current, next, eventType), next.NextStep)
		if err := s.putCAS(ctx, t, current.Revision, next); err != nil {
			return err
		}
		out = next
		return s.projectTask(ctx, t, next, actor)
	})
	return out, err
}

func (s *Service) AddDependency(ctx context.Context, id, dependsOn, actor string) (Task, error) {
	return s.changeDependency(ctx, id, dependsOn, actor, true)
}
func (s *Service) RemoveDependency(ctx context.Context, id, dependsOn, actor string) (Task, error) {
	return s.changeDependency(ctx, id, dependsOn, actor, false)
}
func (s *Service) changeDependency(ctx context.Context, id, dep, actor string, add bool) (Task, error) {
	var out Task
	err := s.withLock(ctx, actor, func(t *tables) error {
		current, ok, err := t.getTask(ctx, id)
		if err != nil {
			return err
		}
		if !ok {
			return ErrNotFound
		}
		if current.Status != StatusOpen {
			return fmt.Errorf("dependencies can change only while a task is open")
		}
		deps := append([]string(nil), current.DependsOn...)
		if add {
			deps = append(deps, dep)
		} else {
			deps = remove(deps, dep)
		}
		deps = uniqueSorted(deps)
		if err := s.validateDependencies(ctx, t, id, deps); err != nil {
			return err
		}
		next := current
		next.DependsOn = deps
		next.Revision++
		next.UpdatedAt = stamp(s.now().UTC())
		kind := "dependency_removed"
		if add {
			kind = "dependency_added"
		}
		next.LastEvent = newEvent(next, kind, actor, current.Status, next.Status, dep, next.NextStep)
		if err := s.putCAS(ctx, t, current.Revision, next); err != nil {
			return err
		}
		out = next
		return s.projectTask(ctx, t, next, actor)
	})
	return out, err
}

func (s *Service) Get(ctx context.Context, id string) (Detail, error) {
	var out Detail
	err := s.withTables(ctx, func(t *tables) error {
		v, ok, err := t.getTask(ctx, id)
		if err != nil {
			return err
		}
		if !ok {
			return ErrNotFound
		}
		all, err := t.allTasks(ctx)
		if err != nil {
			return err
		}
		decorate(&v, indexTasks(all))
		v.ClaimToken = ""
		events, err := t.eventsFor(ctx, id)
		if err != nil {
			return err
		}
		comments, err := t.commentsFor(ctx, id)
		if err != nil {
			return err
		}
		revisions, err := t.specRevisionsFor(ctx, id)
		if err != nil {
			return err
		}
		out = Detail{Task: v, Events: events, Comments: comments, SpecRevisions: revisions}
		return nil
	})
	return out, err
}

func (s *Service) Export(ctx context.Context, id string) (ExportDocument, error) {
	id = strings.TrimSpace(id)
	out := ExportDocument{
		SchemaVersion: ExportSchemaVersion,
		ProjectID:     s.projectID,
		TaskID:        id,
		Tasks:         []Task{},
		Dependencies:  []DependencyRecord{},
		Checks:        []CheckRecord{},
		Events:        []Event{},
		Comments:      []Comment{},
		SpecRevisions: []SpecRevision{},
	}
	err := s.withTables(ctx, func(t *tables) error {
		all, err := t.allTasks(ctx)
		if err != nil {
			return err
		}
		byID := indexTasks(all)
		if id != "" {
			if _, ok := byID[id]; !ok {
				return ErrNotFound
			}
		}
		included := make(map[string]bool, len(all))
		if id == "" {
			for _, task := range all {
				included[task.ID] = true
			}
		} else {
			included[id] = true
			for changed := true; changed; {
				changed = false
				for _, task := range all {
					if !included[task.ID] && included[task.ParentID] {
						included[task.ID] = true
						changed = true
					}
				}
			}
		}

		checksByTask := make(map[string]map[string]Check, len(included))
		for _, task := range all {
			if !included[task.ID] {
				continue
			}
			decorate(&task, byID)
			task.ClaimToken = ""
			out.Tasks = append(out.Tasks, task)
			checksByTask[task.ID] = make(map[string]Check, len(task.Checks))
			for _, check := range task.Checks {
				checksByTask[task.ID][check.ID] = check
			}
		}

		dependencyRows, err := t.allProjectionRows(ctx, t.dependencies)
		if err != nil {
			return err
		}
		for _, row := range dependencyRows {
			record := dependencyRecordFromRow(row)
			if included[record.TaskID] {
				out.Dependencies = append(out.Dependencies, record)
			}
		}
		checkRows, err := t.allProjectionRows(ctx, t.checks)
		if err != nil {
			return err
		}
		for _, row := range checkRows {
			record := checkRecordFromRow(row)
			if !included[record.TaskID] {
				continue
			}
			if check, ok := checksByTask[record.TaskID][record.ID]; ok {
				record.Check = check
			}
			out.Checks = append(out.Checks, record)
		}
		eventRows, err := t.allProjectionRows(ctx, t.events)
		if err != nil {
			return err
		}
		for _, row := range eventRows {
			event := eventFromRow(row)
			if included[event.TaskID] {
				out.Events = append(out.Events, event)
			}
		}
		commentRows, err := t.allProjectionRows(ctx, t.comments)
		if err != nil {
			return err
		}
		for _, row := range commentRows {
			comment := commentFromRow(row)
			if included[comment.TaskID] {
				out.Comments = append(out.Comments, comment)
			}
		}
		revisionRows, err := t.allProjectionRows(ctx, t.specRevisions)
		if err != nil {
			return err
		}
		for _, row := range revisionRows {
			revision := specRevisionFromRow(row)
			if included[revision.TaskID] {
				out.SpecRevisions = append(out.SpecRevisions, revision)
			}
		}

		sort.Slice(out.Dependencies, func(i, j int) bool { return out.Dependencies[i].Key < out.Dependencies[j].Key })
		sort.Slice(out.Checks, func(i, j int) bool {
			if out.Checks[i].TaskID != out.Checks[j].TaskID {
				return out.Checks[i].TaskID < out.Checks[j].TaskID
			}
			return out.Checks[i].ID < out.Checks[j].ID
		})
		sort.Slice(out.Events, func(i, j int) bool {
			if out.Events[i].TaskID != out.Events[j].TaskID {
				return out.Events[i].TaskID < out.Events[j].TaskID
			}
			if out.Events[i].Sequence != out.Events[j].Sequence {
				return out.Events[i].Sequence < out.Events[j].Sequence
			}
			return out.Events[i].Key < out.Events[j].Key
		})
		sort.Slice(out.Comments, func(i, j int) bool {
			if out.Comments[i].TaskID != out.Comments[j].TaskID {
				return out.Comments[i].TaskID < out.Comments[j].TaskID
			}
			if out.Comments[i].Sequence != out.Comments[j].Sequence {
				return out.Comments[i].Sequence < out.Comments[j].Sequence
			}
			return out.Comments[i].ID < out.Comments[j].ID
		})
		sort.Slice(out.SpecRevisions, func(i, j int) bool {
			if out.SpecRevisions[i].TaskID != out.SpecRevisions[j].TaskID {
				return out.SpecRevisions[i].TaskID < out.SpecRevisions[j].TaskID
			}
			if out.SpecRevisions[i].SourceRevision != out.SpecRevisions[j].SourceRevision {
				return out.SpecRevisions[i].SourceRevision < out.SpecRevisions[j].SourceRevision
			}
			return out.SpecRevisions[i].Key < out.SpecRevisions[j].Key
		})
		return nil
	})
	return out, err
}

func (s *Service) Catalog(ctx context.Context, opts CatalogOptions) ([]CatalogItem, error) {
	query := strings.ToLower(strings.TrimSpace(opts.Query))
	status := strings.TrimSpace(opts.Status)
	if status != "" && status != "blocked" && status != "flagged" && !ValidStatus(status) {
		return nil, fmt.Errorf("invalid task status %q", status)
	}
	out := []CatalogItem{}
	err := s.withTables(ctx, func(t *tables) error {
		all, err := t.catalogTasks(ctx)
		if err != nil {
			return err
		}
		byID := indexTasks(all)
		selected := make([]Task, 0, len(all))
		for _, task := range all {
			decorate(&task, byID)
			if status == "blocked" && (task.Status != StatusOpen || len(task.BlockedBy) == 0) {
				continue
			}
			if status == "flagged" && !task.Flagged {
				continue
			}
			if status != "" && status != "blocked" && status != "flagged" && string(task.Status) != status {
				continue
			}
			if query != "" && !strings.Contains(strings.ToLower(strings.Join([]string{
				task.ID, task.Title, task.Description, task.Type, task.Owner,
				task.ProgressSummary, task.NextStep, task.FlagReason,
			}, "\n")), query) {
				continue
			}
			selected = append(selected, task)
		}
		sortCatalogTasks(selected)
		for _, task := range selected {
			out = append(out, CatalogItem{
				ID: task.ID, Title: task.Title, Type: task.Type, Status: task.Status,
				Priority: task.Priority, Owner: task.Owner, Flagged: task.Flagged,
				Ready: task.Ready, BlockedBy: task.BlockedBy, UpdatedAt: task.UpdatedAt,
			})
		}
		return nil
	})
	return out, err
}

func (s *Service) List(ctx context.Context, opts ListOptions) ([]Task, error) {
	var out []Task
	err := s.withTables(ctx, func(t *tables) error {
		all, err := t.allTasks(ctx)
		if err != nil {
			return err
		}
		byID := indexTasks(all)
		for _, v := range all {
			decorate(&v, byID)
			v.ClaimToken = ""
			if opts.Status != "" {
				if opts.Status == "blocked" && (len(v.BlockedBy) == 0 || v.Status != StatusOpen) {
					continue
				}
				if opts.Status == "flagged" && !v.Flagged {
					continue
				}
				if opts.Status != "blocked" && opts.Status != "flagged" && string(v.Status) != opts.Status {
					continue
				}
			}
			if opts.Owner != "" && v.Owner != opts.Owner {
				continue
			}
			if opts.ParentID != "" && v.ParentID != opts.ParentID {
				continue
			}
			if opts.Ready && !v.Ready {
				continue
			}
			out = append(out, v)
		}
		sortTasks(out)
		return nil
	})
	return out, err
}

func (s *Service) Search(ctx context.Context, query string, limit int) ([]SearchResult, error) {
	query = strings.TrimSpace(query)
	if query == "" {
		return nil, errors.New("task search query is required")
	}
	if limit <= 0 {
		limit = 20
	}
	var out []SearchResult
	err := s.withTables(ctx, func(t *tables) error {
		hits, err := t.tasks.Search(ctx, lancestore.Query{Text: query, TextColumn: "search_text", Limit: limit})
		if err != nil {
			return err
		}
		all, err := t.allTasks(ctx)
		if err != nil {
			return err
		}
		byID := indexTasks(all)
		seen := make(map[string]bool, limit)
		for _, hit := range hits {
			v := taskFromRow(hit.Row)
			decorate(&v, byID)
			out = append(out, SearchResult{ID: v.ID, Title: v.Title, Status: v.Status, Priority: v.Priority, Ready: v.Ready, Flagged: v.Flagged, Score: hit.Score})
			seen[v.ID] = true
		}
		commentHits, err := t.comments.Search(ctx, lancestore.Query{Text: query, TextColumn: "body", Limit: limit})
		if err != nil {
			return err
		}
		for _, hit := range commentHits {
			id := text(hit.Row, "task_id")
			if seen[id] || len(out) >= limit {
				continue
			}
			v, ok := byID[id]
			if !ok {
				continue
			}
			decorate(&v, byID)
			out = append(out, SearchResult{ID: v.ID, Title: v.Title, Status: v.Status, Priority: v.Priority, Ready: v.Ready, Flagged: v.Flagged, Score: hit.Score})
			seen[id] = true
		}
		return nil
	})
	return out, err
}

func (s *Service) putCAS(ctx context.Context, t *tables, expected int64, v Task) error {
	res, err := t.tasks.Merge(ctx, lancestore.MergeOptions{KeyColumn: "id", MatchCondition: fmt.Sprintf("target.revision = %d", expected)}, []lancestore.Row{taskRow(v)})
	if err != nil {
		return err
	}
	if !res.Changed() {
		return fmt.Errorf("%w: %s; retry", ErrConcurrent, v.ID)
	}
	return nil
}

func (s *Service) validateDependencies(ctx context.Context, t *tables, id string, deps []string) error {
	all, err := t.allTasks(ctx)
	if err != nil {
		return err
	}
	byID := indexTasks(all)
	for _, dep := range deps {
		if dep == id {
			return errors.New("a task cannot depend on itself")
		}
		if _, ok := byID[dep]; !ok {
			return fmt.Errorf("dependency %s: %w", dep, ErrNotFound)
		}
		if pending, err := removalPending(ctx, t, dep); err != nil {
			return err
		} else if pending {
			return fmt.Errorf("dependency %s is pending removal", dep)
		}
		if reaches(byID, dep, id, map[string]bool{}) {
			return fmt.Errorf("dependency %s creates a cycle", dep)
		}
	}
	return nil
}

func removalPending(ctx context.Context, t *tables, id string) (bool, error) {
	hits, err := t.control.Search(ctx, lancestore.Query{Filter: "key = " + quote(taskRemovalPrefix+id), Limit: 1})
	return len(hits) > 0, err
}

func taskReference(all []Task, id string) string {
	for _, candidate := range all {
		if candidate.ID == id {
			continue
		}
		if candidate.ParentID == id {
			return "subtask " + candidate.ID
		}
		for _, dependency := range candidate.DependsOn {
			if dependency == id {
				return "dependent " + candidate.ID
			}
		}
	}
	return ""
}

func reaches(all map[string]Task, from, want string, seen map[string]bool) bool {
	if from == want {
		return true
	}
	if seen[from] {
		return false
	}
	seen[from] = true
	for _, d := range all[from].DependsOn {
		if reaches(all, d, want, seen) {
			return true
		}
	}
	return false
}

func (s *Service) reconcileLocked(ctx context.Context, t *tables, actor string) error {
	controlRows, err := t.allProjectionRows(ctx, t.control)
	if err != nil {
		return err
	}
	all, err := t.allTasks(ctx)
	if err != nil {
		return err
	}
	for _, row := range controlRows {
		key := text(row, "key")
		if !strings.HasPrefix(key, taskRemovalPrefix) {
			continue
		}
		id := strings.TrimPrefix(key, taskRemovalPrefix)
		if reference := taskReference(all, id); reference != "" {
			if err := t.control.DeleteByKey(ctx, "key", []string{key}); err != nil {
				return err
			}
			continue
		}
		if err := finishTaskRemoval(ctx, t, id); err != nil {
			return err
		}
	}
	all, err = t.allTasks(ctx)
	if err != nil {
		return err
	}
	now := stamp(s.now().UTC())
	for _, v := range all {
		if v.Status == StatusInProgress && v.LeaseExpiresAt != "" && v.LeaseExpiresAt <= now {
			old := v
			v.Status = StatusOpen
			clearClaim(&v)
			v.Revision++
			v.UpdatedAt = now
			v.LastEvent = newEvent(v, "lease_expired", "system", old.Status, v.Status, "claim expired; task is ready for takeover", v.NextStep)
			if err := s.putCAS(ctx, t, old.Revision, v); err != nil {
				if errors.Is(err, ErrConcurrent) {
					continue
				}
				return err
			}
		}
		if v.Status == StatusCompleted {
			if violations := completionViolations(v, all); v.Flagged || len(violations) > 0 {
				old := v
				v.Status = StatusOpen
				v.CompletedAt = ""
				v.CompletedBy = ""
				v.Revision++
				v.UpdatedAt = now
				reasons := violations
				if v.Flagged {
					reasons = append(reasons, "task is flagged")
				}
				v.LastEvent = newEvent(v, "completion_invalidated", "hook", old.Status, v.Status, strings.Join(reasons, "; "), v.NextStep)
				if err := s.putCAS(ctx, t, old.Revision, v); err != nil {
					if errors.Is(err, ErrConcurrent) {
						continue
					}
					return err
				}
			}
		}
	}

	if err := t.refresh(ctx); err != nil {
		return err
	}
	all, err = t.allTasks(ctx)
	if err != nil {
		return err
	}
	return s.projectAllTasks(ctx, t, all, actor)
}

func (s *Service) projectAllTasks(ctx context.Context, t *tables, all []Task, actor string) error {
	eventRows, err := t.allProjectionRows(ctx, t.events)
	if err != nil {
		return err
	}
	existingEvents := make(map[string]bool, len(eventRows))
	for _, row := range eventRows {
		existingEvents[text(row, "key")] = true
	}
	missingEvents := make([]lancestore.Row, 0)
	for _, v := range all {
		if v.LastEvent.Key != "" && !existingEvents[v.LastEvent.Key] {
			missingEvents = append(missingEvents, eventRow(v.LastEvent))
		}
	}
	if len(missingEvents) > 0 {
		if _, err := t.events.Merge(ctx, lancestore.MergeOptions{KeyColumn: "key", MatchCondition: "false", InsertIfMissing: true}, missingEvents); err != nil {
			return err
		}
	}

	commentRows, err := t.allProjectionRows(ctx, t.comments)
	if err != nil {
		return err
	}
	existingComments := make(map[string]bool, len(commentRows))
	for _, row := range commentRows {
		existingComments[text(row, "id")] = true
	}
	missingComments := make([]lancestore.Row, 0)
	for _, v := range all {
		if v.LastComment.ID != "" && !existingComments[v.LastComment.ID] {
			missingComments = append(missingComments, commentRow(v.LastComment))
		}
	}
	if len(missingComments) > 0 {
		if _, err := t.comments.Merge(ctx, lancestore.MergeOptions{KeyColumn: "id", MatchCondition: "false", InsertIfMissing: true}, missingComments); err != nil {
			return err
		}
	}

	existingRevisions, err := t.allProjectionRows(ctx, t.specRevisions)
	if err != nil {
		return err
	}
	revisionKeys := make(map[string]bool, len(existingRevisions))
	for _, row := range existingRevisions {
		revisionKeys[text(row, "key")] = true
	}
	missingRevisions := make([]lancestore.Row, 0)
	for _, v := range all {
		if revision := v.LastEvent.SpecRevision; revision != nil && !revisionKeys[revision.Key] {
			missingRevisions = append(missingRevisions, specRevisionRow(*revision))
		}
	}
	if len(missingRevisions) > 0 {
		if _, err := t.specRevisions.Merge(ctx, lancestore.MergeOptions{KeyColumn: "key", MatchCondition: "false", InsertIfMissing: true}, missingRevisions); err != nil {
			return err
		}
	}

	if err := s.projectAllDependencies(ctx, t, all, actor); err != nil {
		return err
	}
	return s.projectAllChecks(ctx, t, all)
}

func (s *Service) projectAllDependencies(ctx context.Context, t *tables, all []Task, actor string) error {
	rows, err := t.allProjectionRows(ctx, t.dependencies)
	if err != nil {
		return err
	}
	existing := make(map[string]lancestore.Row, len(rows))
	for _, row := range rows {
		existing[text(row, "key")] = row
	}
	wanted := make(map[string]bool)
	taskRevisions := make(map[string]int64, len(all))
	changes := make([]lancestore.Row, 0)
	now := stamp(s.now().UTC())
	for _, v := range all {
		taskRevisions[v.ID] = v.Revision
		for _, dependency := range v.DependsOn {
			key := v.ID + "/" + dependency
			wanted[key] = true
			old, ok := existing[key]
			if ok && boolValue(old, "active") && text(old, "task_id") == v.ID && text(old, "depends_on") == dependency {
				continue
			}
			changes = append(changes, lancestore.Row{"key": key, "task_id": v.ID, "depends_on": dependency, "active": true, "created_at": now, "created_by": actor, "revision": v.Revision})
		}
	}
	for _, row := range rows {
		key := text(row, "key")
		revision, knownTask := taskRevisions[text(row, "task_id")]
		if wanted[key] || !knownTask || !boolValue(row, "active") {
			continue
		}
		row["active"] = false
		row["revision"] = revision
		changes = append(changes, row)
	}
	if len(changes) == 0 {
		return nil
	}
	_, err = t.dependencies.Merge(ctx, lancestore.MergeOptions{KeyColumn: "key", MatchCondition: "target.revision <= source.revision", InsertIfMissing: true}, changes)
	return err
}

func (s *Service) projectAllChecks(ctx context.Context, t *tables, all []Task) error {
	rows, err := t.allProjectionRows(ctx, t.checks)
	if err != nil {
		return err
	}
	existing := make(map[string]lancestore.Row, len(rows))
	for _, row := range rows {
		existing[text(row, "key")] = row
	}
	wanted := make(map[string]bool)
	taskRevisions := make(map[string]int64, len(all))
	changes := make([]lancestore.Row, 0)
	for _, v := range all {
		taskRevisions[v.ID] = v.Revision
		for _, check := range v.Checks {
			wanted[check.ID] = true
			active := check.Status != "superseded"
			row := lancestore.Row{"key": check.ID, "task_id": v.ID, "kind": check.Kind, "text": check.Text,
				"status": check.Status, "evidence": check.Evidence, "verified_by": check.VerifiedBy,
				"verified_at": check.VerifiedAt, "active": active, "revision": v.Revision}
			old, ok := existing[check.ID]
			if ok && boolValue(old, "active") == active && text(old, "task_id") == v.ID &&
				text(old, "kind") == check.Kind && text(old, "text") == check.Text &&
				text(old, "status") == check.Status && text(old, "evidence") == check.Evidence &&
				text(old, "verified_by") == check.VerifiedBy && text(old, "verified_at") == check.VerifiedAt {
				continue
			}
			changes = append(changes, row)
		}
	}
	for _, row := range rows {
		key := text(row, "key")
		revision, knownTask := taskRevisions[text(row, "task_id")]
		if wanted[key] || !knownTask || !boolValue(row, "active") {
			continue
		}
		row["active"] = false
		row["revision"] = revision
		changes = append(changes, row)
	}
	if len(changes) == 0 {
		return nil
	}
	_, err = t.checks.Merge(ctx, lancestore.MergeOptions{KeyColumn: "key", MatchCondition: "target.revision <= source.revision", InsertIfMissing: true}, changes)
	return err
}

func (s *Service) projectTask(ctx context.Context, t *tables, v Task, actor string) error {
	if err := ensureEvent(ctx, t, v.LastEvent); err != nil {
		return err
	}
	if v.LastEvent.SpecRevision != nil {
		if err := ensureSpecRevision(ctx, t, *v.LastEvent.SpecRevision); err != nil {
			return err
		}
	}
	if err := ensureComment(ctx, t, v.LastComment); err != nil {
		return err
	}
	if err := s.projectDependencies(ctx, t, v, actor); err != nil {
		return err
	}
	return s.projectChecks(ctx, t, v)
}

func ensureEvent(ctx context.Context, t *tables, event Event) error {
	if event.Key == "" {
		return nil
	}
	exists, err := projectionExists(ctx, t.events, "key", event.Key)
	if err != nil || exists {
		return err
	}
	_, err = t.events.Merge(ctx, lancestore.MergeOptions{
		KeyColumn: "key", MatchCondition: "false", InsertIfMissing: true,
	}, []lancestore.Row{eventRow(event)})
	return err
}

func ensureComment(ctx context.Context, t *tables, comment Comment) error {
	if comment.ID == "" {
		return nil
	}
	exists, err := projectionExists(ctx, t.comments, "id", comment.ID)
	if err != nil || exists {
		return err
	}
	_, err = t.comments.Merge(ctx, lancestore.MergeOptions{
		KeyColumn: "id", MatchCondition: "false", InsertIfMissing: true,
	}, []lancestore.Row{commentRow(comment)})
	return err
}

func ensureSpecRevision(ctx context.Context, t *tables, revision SpecRevision) error {
	if revision.Key == "" {
		return nil
	}
	exists, err := projectionExists(ctx, t.specRevisions, "key", revision.Key)
	if err != nil || exists {
		return err
	}
	_, err = t.specRevisions.Merge(ctx, lancestore.MergeOptions{
		KeyColumn: "key", MatchCondition: "false", InsertIfMissing: true,
	}, []lancestore.Row{specRevisionRow(revision)})
	return err
}

func projectionExists(ctx context.Context, table *lancestore.Table, column, key string) (bool, error) {
	hits, err := table.Search(ctx, lancestore.Query{Filter: column + " = " + quote(key), Limit: 1})
	return len(hits) > 0, err
}

func (s *Service) projectChecks(ctx context.Context, t *tables, v Task) error {
	hits, err := t.checks.Search(ctx, lancestore.Query{Filter: "task_id = " + quote(v.ID), Limit: pageSize})
	if err != nil {
		return err
	}
	existing := make(map[string]lancestore.Row, len(hits))
	for _, hit := range hits {
		existing[text(hit.Row, "key")] = hit.Row
	}
	wanted := make(map[string]bool, len(v.Checks))
	var rows []lancestore.Row
	for _, check := range v.Checks {
		wanted[check.ID] = true
		active := check.Status != "superseded"
		row := lancestore.Row{"key": check.ID, "task_id": v.ID, "kind": check.Kind, "text": check.Text,
			"status": check.Status, "evidence": check.Evidence, "verified_by": check.VerifiedBy,
			"verified_at": check.VerifiedAt, "active": active, "revision": v.Revision}
		if old, ok := existing[check.ID]; ok && boolValue(old, "active") == active &&
			text(old, "status") == check.Status && text(old, "evidence") == check.Evidence &&
			text(old, "verified_by") == check.VerifiedBy && text(old, "verified_at") == check.VerifiedAt {
			continue
		}
		rows = append(rows, row)
	}
	for _, hit := range hits {
		key := text(hit.Row, "key")
		if wanted[key] || !boolValue(hit.Row, "active") {
			continue
		}
		row := hit.Row
		row["active"] = false
		row["revision"] = v.Revision
		rows = append(rows, row)
	}
	if len(rows) == 0 {
		return nil
	}
	return t.checks.Upsert(ctx, "key", rows)
}

func (s *Service) projectDependencies(ctx context.Context, t *tables, v Task, actor string) error {
	hits, err := t.dependencies.Search(ctx, lancestore.Query{Filter: "task_id = " + quote(v.ID), Limit: pageSize})
	if err != nil {
		return err
	}
	wanted := map[string]bool{}
	existing := make(map[string]lancestore.Row, len(hits))
	for _, h := range hits {
		existing[text(h.Row, "key")] = h.Row
	}
	rows := []lancestore.Row{}
	now := stamp(s.now().UTC())
	for _, d := range v.DependsOn {
		key := v.ID + "/" + d
		wanted[key] = true
		if row, ok := existing[key]; ok && boolValue(row, "active") {
			continue
		}
		rows = append(rows, lancestore.Row{"key": key, "task_id": v.ID, "depends_on": d, "active": true, "created_at": now, "created_by": actor, "revision": v.Revision})
	}
	for _, h := range hits {
		key := text(h.Row, "key")
		if !wanted[key] {
			r := h.Row
			r["active"] = false
			r["revision"] = v.Revision
			rows = append(rows, r)
		}
	}
	if len(rows) > 0 {
		return t.dependencies.Upsert(ctx, "key", rows)
	}
	return nil
}

func newEvent(v Task, kind, actor string, from, to Status, summary, next string) Event {
	return Event{Key: fmt.Sprintf("%s/%020d", v.ID, v.Revision), TaskID: v.ID, Sequence: v.Revision, Type: kind, Actor: actor, At: v.UpdatedAt, FromStatus: from, ToStatus: to, Summary: summary, NextStep: next, Revision: v.Revision}
}

func mutationSummary(before, after Task, eventType string) string {
	switch eventType {
	case "flagged":
		return after.FlagReason
	case "unflagged":
		return before.FlagReason
	case "check_passed", "check_failed":
		for _, next := range after.Checks {
			for _, previous := range before.Checks {
				if next.ID == previous.ID && (next.Status != previous.Status || next.Evidence != previous.Evidence) {
					return next.ID + ": " + next.Evidence
				}
			}
		}
	}
	return after.ProgressSummary
}

func clearClaim(v *Task) {
	v.Owner = ""
	v.ClaimToken = ""
	v.ClaimedAt = ""
	v.LeaseExpiresAt = ""
	v.HeartbeatAt = ""
}

const timestampLayout = "2006-01-02T15:04:05.000000000Z"

func stamp(v time.Time) string { return v.UTC().Format(timestampLayout) }
func renewedLeaseExpiry(current string, now time.Time, lease time.Duration) string {
	next := stamp(now.Add(lease))
	if current > next {
		return current
	}
	return next
}
func canonicalKey(v string) string { return strings.ToLower(strings.Join(strings.Fields(v), "-")) }
func taskIDDigest(project, key string) string {
	sum := sha256.Sum256([]byte(project + "\x00" + key))
	return fmt.Sprintf("%x", sum[:])
}

func deterministicCheckID(taskID, kind, text string) string {
	sum := sha256.Sum256([]byte(taskID + "\x00" + kind + "\x00" + canonicalKey(text)))
	return fmt.Sprintf("chk-%x", sum[:6])
}

func deterministicCommentID(taskID, key string) string {
	sum := sha256.Sum256([]byte(taskID + "\x00" + key))
	return fmt.Sprintf("cmt-%x", sum[:6])
}

func nonEmpty(in []string) []string {
	out := make([]string, 0, len(in))
	for _, value := range in {
		if value = strings.TrimSpace(value); value != "" {
			out = append(out, value)
		}
	}
	return out
}

func buildChecks(taskID string, acceptance, tests []string) ([]Check, error) {
	var out []Check
	seen := map[string]bool{}
	for _, group := range []struct {
		kind   string
		values []string
	}{{"acceptance", acceptance}, {"test", tests}} {
		for _, value := range nonEmpty(group.values) {
			id := deterministicCheckID(taskID, group.kind, value)
			if seen[id] {
				continue
			}
			seen[id] = true
			out = append(out, Check{ID: id, Kind: group.kind, Text: value, Status: "pending"})
		}
	}
	if len(out) == 0 {
		return nil, errors.New("task checks are required")
	}
	sortChecks(out)
	return out, nil
}

func sortChecks(checks []Check) {
	sort.Slice(checks, func(i, j int) bool {
		if checks[i].Kind != checks[j].Kind {
			return checks[i].Kind < checks[j].Kind
		}
		return checks[i].ID < checks[j].ID
	})
}

func taskSpec(v Task) TaskSpec {
	return TaskSpec{
		Title: v.Title, Description: v.Description, Type: v.Type, Priority: v.Priority, ParentID: v.ParentID,
		DependsOn: append([]string(nil), v.DependsOn...), Checks: append([]Check(nil), v.Checks...),
	}
}

func newSpecRevision(before, after Task, kind, actor, reason string) SpecRevision {
	return SpecRevision{
		Key: fmt.Sprintf("%s/%020d", after.ID, after.Revision), TaskID: after.ID, SourceRevision: after.Revision,
		Kind: kind, Actor: actor, Reason: reason, At: after.UpdatedAt, Before: taskSpec(before), After: taskSpec(after),
	}
}

func validateActiveCheckKinds(checks []Check) error {
	kinds := map[string]int{}
	for _, check := range checks {
		if check.Status != "superseded" {
			kinds[check.Kind]++
		}
	}
	if kinds["acceptance"] == 0 || kinds["test"] == 0 {
		return errors.New("task must retain at least one active acceptance check and one active test check")
	}
	return nil
}

func validateParentChange(taskID, parentID string, all []Task) error {
	if parentID == "" {
		return nil
	}
	if parentID == taskID {
		return errors.New("a task cannot be its own parent")
	}
	byID := indexTasks(all)
	parent, ok := byID[parentID]
	if !ok {
		return fmt.Errorf("parent %s: %w", parentID, ErrNotFound)
	}
	if parent.Status == StatusCompleted || parent.Status == StatusCancelled {
		return fmt.Errorf("parent task %s is already %s", parentID, parent.Status)
	}
	seen := map[string]bool{}
	for current := parentID; current != ""; current = byID[current].ParentID {
		if current == taskID {
			return errors.New("task parent relationship would create a cycle")
		}
		if seen[current] {
			return errors.New("existing task parent relationship contains a cycle")
		}
		seen[current] = true
	}
	return nil
}

func validCommentKind(kind string) bool {
	switch kind {
	case "note", "decision", "problem", "lesson", "knowledge":
		return true
	default:
		return false
	}
}

func completionViolations(v Task, all []Task) []string {
	var violations []string
	kinds := map[string]int{}
	for _, check := range v.Checks {
		if check.Status == "superseded" {
			continue
		}
		kinds[check.Kind]++
		if check.Status != "passed" || strings.TrimSpace(check.Evidence) == "" {
			violations = append(violations, fmt.Sprintf("%s %s is %s", check.Kind, check.ID, check.Status))
		}
	}
	if kinds["acceptance"] == 0 {
		violations = append(violations, "task has no acceptance checks")
	}
	if kinds["test"] == 0 {
		violations = append(violations, "task has no test checks")
	}
	byID := indexTasks(all)
	for _, dependencyID := range v.DependsOn {
		dependency, ok := byID[dependencyID]
		if !ok || !effectivelyComplete(dependency, byID, map[string]bool{}) {
			violations = append(violations, fmt.Sprintf("dependency %s is not validly completed", dependencyID))
		}
	}
	for _, child := range all {
		if child.ParentID == v.ID && !effectivelyComplete(child, byID, map[string]bool{}) {
			violations = append(violations, fmt.Sprintf("subtask %s is not validly completed", child.ID))
		}
	}
	sort.Strings(violations)
	return violations
}

func effectivelyComplete(v Task, all map[string]Task, seen map[string]bool) bool {
	if v.Status != StatusCompleted || v.Flagged || seen[v.ID] {
		return false
	}
	seen[v.ID] = true
	kinds := map[string]bool{}
	for _, check := range v.Checks {
		if check.Status == "superseded" {
			continue
		}
		kinds[check.Kind] = true
		if check.Status != "passed" || strings.TrimSpace(check.Evidence) == "" {
			return false
		}
	}
	if !kinds["acceptance"] || !kinds["test"] {
		return false
	}
	for _, child := range all {
		if child.ParentID == v.ID && !effectivelyComplete(child, all, seen) {
			return false
		}
	}
	delete(seen, v.ID)
	return true
}

func uniqueSorted(in []string) []string {
	m := map[string]bool{}
	for _, v := range in {
		v = strings.TrimSpace(v)
		if v != "" {
			m[v] = true
		}
	}
	out := make([]string, 0, len(m))
	for v := range m {
		out = append(out, v)
	}
	sort.Strings(out)
	return out
}
func remove(in []string, want string) []string {
	out := in[:0]
	for _, v := range in {
		if v != want {
			out = append(out, v)
		}
	}
	return out
}
func indexTasks(all []Task) map[string]Task {
	out := make(map[string]Task, len(all))
	for _, v := range all {
		out[v.ID] = v
	}
	return out
}
func blockedBy(v Task, all map[string]Task) []string {
	var out []string
	for _, id := range v.DependsOn {
		dep, ok := all[id]
		if !ok || dep.Status != StatusCompleted {
			out = append(out, id)
		}
	}
	sort.Strings(out)
	return out
}
func decorate(v *Task, all map[string]Task) {
	v.BlockedBy = blockedBy(*v, all)
	v.Ready = v.Status == StatusOpen && len(v.BlockedBy) == 0
}
func sortTasks(v []Task) {
	sort.Slice(v, func(i, j int) bool {
		if v[i].Priority != v[j].Priority {
			return v[i].Priority < v[j].Priority
		}
		if v[i].CreatedAt != v[j].CreatedAt {
			return v[i].CreatedAt < v[j].CreatedAt
		}
		return v[i].ID < v[j].ID
	})
}

func sortCatalogTasks(v []Task) {
	sort.Slice(v, func(i, j int) bool {
		if v[i].CreatedAt != v[j].CreatedAt {
			return v[i].CreatedAt > v[j].CreatedAt
		}
		return v[i].ID < v[j].ID
	})
}
