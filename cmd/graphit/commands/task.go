package commands

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/output"
	graphtask "github.com/graphit-labs/graphit-code/internal/task"
	"github.com/spf13/cobra"
)

func newTaskCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "task", Short: "Manage shared deterministic agent tasks", Long: `Manage project work in the authoritative LanceDB task store.

Open, unclaimed tasks are the backlog. Dependencies determine readiness. Agents
must claim before work, checkpoint progress, and complete or release the claim.
The returned claim token fences stopped or replaced agents from later writes.`}
	cmd.AddCommand(newTaskBatchCmd(), newTaskCreateCmd(), newTaskListCmd(), newTaskGetCmd(), newTaskExportCmd(), newTaskSearchCmd(), newTaskClaimCmd(), newTaskProgressCmd(), newTaskHeartbeatCmd(), newTaskReleaseCmd(), newTaskCompleteCmd(), newTaskCancelCmd(), newTaskRemoveCmd(), newTaskFlagCmd(), newTaskUnflagCmd(), newTaskCheckCmd(), newTaskReviseCmd(), newTaskCommentCmd(), newTaskDependencyCmd(), newModuleRuleCmd("task"))
	return cmd
}

func currentTaskService() (*graphtask.Service, error) {
	dir, err := os.Getwd()
	if err != nil {
		return nil, err
	}
	return graphtask.Open(dir)
}

func printTaskJSON(value any) error {
	data, err := json.MarshalIndent(value, "", "  ")
	if err != nil {
		return err
	}
	output.NewPrinter("").Data(string(data))
	return nil
}

func cliTaskActor(value string) string {
	if value != "" {
		return value
	}
	return graphtask.AgentIDForSession("")
}
func splitTaskIDs(value string) []string {
	var out []string
	for _, v := range strings.Split(value, ",") {
		if v = strings.TrimSpace(v); v != "" {
			out = append(out, v)
		}
	}
	return out
}
func cliTaskLease(value string) (time.Duration, error) {
	if value == "" {
		return graphtask.DefaultLease, nil
	}
	d, err := time.ParseDuration(value)
	if err != nil || d <= 0 {
		return 0, fmt.Errorf("invalid positive lease duration %q", value)
	}
	return d, nil
}

func newTaskBatchCmd() *cobra.Command {
	var actor string
	cmd := &cobra.Command{Use: "batch <file|->", Short: "Run ordered task mutations from JSON", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		var input graphtask.BatchInput
		if err := decodeTaskBatch(cmd.InOrStdin(), args[0], &input); err != nil {
			return err
		}
		input.Actor = cliTaskActor(actor)
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		result, err := svc.Batch(cmd.Context(), input)
		if err != nil {
			return err
		}
		if err := printTaskJSON(result); err != nil {
			return err
		}
		if result.Failed > 0 {
			return fmt.Errorf("task batch completed with %d failed operation(s)", result.Failed)
		}
		return nil
	}}
	cmd.Flags().StringVar(&actor, "agent", "", "Agent identity (defaults to this Graphit unit)")
	return cmd
}

func decodeTaskBatch(stdin io.Reader, source string, input *graphtask.BatchInput) error {
	return decodeStrictTaskJSON(stdin, source, input, "task batch")
}

func decodeStrictTaskJSON(stdin io.Reader, source string, input any, label string) error {
	reader := stdin
	var file *os.File
	if source != "-" {
		var err error
		file, err = os.Open(source)
		if err != nil {
			return fmt.Errorf("opening %s: %w", label, err)
		}
		defer file.Close()
		reader = file
	}
	decoder := json.NewDecoder(reader)
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(input); err != nil {
		return fmt.Errorf("decoding %s: %w", label, err)
	}
	var extra any
	if err := decoder.Decode(&extra); err != io.EOF {
		if err == nil {
			return fmt.Errorf("decoding %s: multiple JSON values are not allowed", label)
		}
		return fmt.Errorf("decoding %s: %w", label, err)
	}
	return nil
}

func newTaskCreateCmd() *cobra.Command {
	var description, kind, deps, key, actor, parentID string
	var acceptance, tests []string
	var priority int
	cmd := &cobra.Command{Use: "create <title>", Short: "Create an open task", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Create(context.Background(), graphtask.CreateInput{Title: args[0], Description: description, AcceptanceCriteria: acceptance, Tests: tests, Type: kind, Priority: priority, ParentID: parentID, DependsOn: splitTaskIDs(deps), IdempotencyKey: key, Actor: cliTaskActor(actor)})
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&description, "description", "", "Robust self-contained task specification")
	_ = cmd.MarkFlagRequired("description")
	cmd.Flags().StringSliceVar(&acceptance, "acceptance", nil, "Observable acceptance criterion (repeat or comma-separate)")
	_ = cmd.MarkFlagRequired("acceptance")
	cmd.Flags().StringSliceVar(&tests, "test", nil, "Concrete test or validation (repeat or comma-separate)")
	_ = cmd.MarkFlagRequired("test")
	cmd.Flags().StringVar(&kind, "type", "task", "Task type")
	cmd.Flags().IntVarP(&priority, "priority", "p", 2, "Priority 0-4")
	cmd.Flags().StringVar(&deps, "depends-on", "", "Comma-separated blocking task IDs")
	cmd.Flags().StringVar(&parentID, "parent", "", "Parent task ID for a subtask")
	cmd.Flags().StringVar(&key, "idempotency-key", "", "Stable caller key (defaults to canonical title)")
	cmd.Flags().StringVar(&actor, "agent", "", "Agent identity (defaults to this Graphit unit)")
	return cmd
}

func newTaskListCmd() *cobra.Command {
	var status, owner, parentID string
	var ready bool
	cmd := &cobra.Command{Use: "list", Aliases: []string{"ready"}, Short: "List tasks or ready work", RunE: func(cmd *cobra.Command, args []string) error {
		if cmd.CalledAs() == "ready" {
			ready = true
		}
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.List(context.Background(), graphtask.ListOptions{Status: status, Owner: owner, ParentID: parentID, Ready: ready})
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&status, "status", "", "Status filter, including derived blocked")
	cmd.Flags().StringVar(&owner, "owner", "", "Exact owner filter")
	cmd.Flags().StringVar(&parentID, "parent", "", "Only direct subtasks of this task ID")
	cmd.Flags().BoolVar(&ready, "ready", false, "Only dependency-ready open tasks")
	return cmd
}
func newTaskGetCmd() *cobra.Command {
	return &cobra.Command{Use: "get <id>", Aliases: []string{"show"}, Short: "Show a task and audit history", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Get(context.Background(), args[0])
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
}

func newTaskExportCmd() *cobra.Command {
	return &cobra.Command{Use: "export [id]", Short: "Export complete task data as JSON", Args: cobra.MaximumNArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		id := ""
		if len(args) == 1 {
			id = args[0]
		}
		value, err := svc.Export(cmd.Context(), id)
		if err != nil {
			return err
		}
		return printTaskJSON(value)
	}}
}

func newTaskSearchCmd() *cobra.Command {
	var limit int
	cmd := &cobra.Command{Use: "search <query>", Short: "Search current and prior tasks", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Search(context.Background(), args[0], limit)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().IntVar(&limit, "limit", 20, "Maximum results")
	return cmd
}

func newTaskClaimCmd() *cobra.Command {
	var actor, leaseText string
	cmd := &cobra.Command{Use: "claim <id>", Short: "Atomically claim ready work", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		lease, err := cliTaskLease(leaseText)
		if err != nil {
			return err
		}
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Claim(context.Background(), args[0], cliTaskActor(actor), lease)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&actor, "agent", "", "Agent identity")
	cmd.Flags().StringVar(&leaseText, "lease", graphtask.DefaultLease.String(), "Claim lease duration")
	return cmd
}

func claimFlags(cmd *cobra.Command, token, actor, lease *string) {
	cmd.Flags().StringVar(token, "claim-token", "", "Fencing token returned by claim")
	_ = cmd.MarkFlagRequired("claim-token")
	cmd.Flags().StringVar(actor, "agent", "", "Agent identity")
	if lease != nil {
		cmd.Flags().StringVar(lease, "lease", graphtask.DefaultLease.String(), "Renewed lease duration")
	}
}

func newTaskProgressCmd() *cobra.Command {
	var token, actor, summary, next, leaseText string
	cmd := &cobra.Command{Use: "progress <id>", Short: "Record a resumable checkpoint", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		lease, err := cliTaskLease(leaseText)
		if err != nil {
			return err
		}
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Progress(context.Background(), args[0], token, cliTaskActor(actor), summary, next, lease)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&summary, "summary", "", "What landed or was verified")
	_ = cmd.MarkFlagRequired("summary")
	cmd.Flags().StringVar(&next, "next-step", "", "Exact continuation step")
	claimFlags(cmd, &token, &actor, &leaseText)
	return cmd
}
func newTaskHeartbeatCmd() *cobra.Command {
	var token, actor, leaseText string
	cmd := &cobra.Command{Use: "heartbeat <id>", Short: "Renew a task lease", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		lease, err := cliTaskLease(leaseText)
		if err != nil {
			return err
		}
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Heartbeat(context.Background(), args[0], token, cliTaskActor(actor), lease)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	claimFlags(cmd, &token, &actor, &leaseText)
	return cmd
}
func newTaskReleaseCmd() *cobra.Command {
	var token, actor, summary, next string
	cmd := &cobra.Command{Use: "release <id>", Short: "Release work for immediate takeover", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Release(context.Background(), args[0], token, cliTaskActor(actor), summary, next)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&summary, "summary", "", "Last completed work or blocker")
	cmd.Flags().StringVar(&next, "next-step", "", "Exact continuation step")
	claimFlags(cmd, &token, &actor, nil)
	return cmd
}
func newTaskCompleteCmd() *cobra.Command {
	var token, actor, summary string
	cmd := &cobra.Command{Use: "complete <id>", Aliases: []string{"close"}, Short: "Complete claimed work", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Complete(context.Background(), args[0], token, cliTaskActor(actor), summary)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&summary, "summary", "", "Acceptance evidence and result")
	claimFlags(cmd, &token, &actor, nil)
	return cmd
}

func newTaskCancelCmd() *cobra.Command {
	var token, actor, reason string
	cmd := &cobra.Command{Use: "cancel <id>", Short: "Cancel work with an audited reason", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Cancel(context.Background(), args[0], token, cliTaskActor(actor), reason)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&reason, "reason", "", "Why the task is no longer needed")
	_ = cmd.MarkFlagRequired("reason")
	cmd.Flags().StringVar(&token, "claim-token", "", "Required fencing token when the task is in progress")
	cmd.Flags().StringVar(&actor, "agent", "", "Agent identity")
	return cmd
}

func newTaskRemoveCmd() *cobra.Command {
	var confirmation, actor, reason string
	cmd := &cobra.Command{Use: "remove <id>", Aliases: []string{"rm"}, Short: "Hard-remove certainly obsolete work", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Remove(context.Background(), args[0], confirmation, cliTaskActor(actor), reason)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&confirmation, "confirm", "", "Exact task ID confirming hard deletion")
	_ = cmd.MarkFlagRequired("confirm")
	cmd.Flags().StringVar(&reason, "reason", "", "Why hard deletion is certainly correct")
	_ = cmd.MarkFlagRequired("reason")
	cmd.Flags().StringVar(&actor, "agent", "", "Agent identity")
	return cmd
}

func newTaskFlagCmd() *cobra.Command {
	var token, actor, reason string
	cmd := &cobra.Command{Use: "flag <id>", Short: "Prevent completion with a recorded reason", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Flag(context.Background(), args[0], token, cliTaskActor(actor), reason)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&reason, "reason", "", "Reason completion is gated")
	_ = cmd.MarkFlagRequired("reason")
	claimFlags(cmd, &token, &actor, nil)
	return cmd
}

func newTaskUnflagCmd() *cobra.Command {
	var token, actor string
	cmd := &cobra.Command{Use: "unflag <id>", Short: "Remove a resolved completion gate", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Unflag(context.Background(), args[0], token, cliTaskActor(actor))
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	claimFlags(cmd, &token, &actor, nil)
	return cmd
}

func newTaskCheckCmd() *cobra.Command {
	var token, actor, evidence, leaseText string
	var failed bool
	cmd := &cobra.Command{Use: "check <task-id> <check-id>", Short: "Record acceptance or test evidence", Args: cobra.ExactArgs(2), RunE: func(cmd *cobra.Command, args []string) error {
		lease, err := cliTaskLease(leaseText)
		if err != nil {
			return err
		}
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.VerifyCheck(context.Background(), args[0], token, cliTaskActor(actor), args[1], !failed, evidence, lease)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&evidence, "evidence", "", "Concrete command output, observation, or artifact")
	_ = cmd.MarkFlagRequired("evidence")
	cmd.Flags().BoolVar(&failed, "failed", false, "Record this check as failed instead of passed")
	claimFlags(cmd, &token, &actor, &leaseText)
	cmd.AddCommand(newTaskCheckSupersedeCmd())
	return cmd
}

func newTaskCheckSupersedeCmd() *cobra.Command {
	var token, actor, reason, replacement, replacementKind, leaseText string
	var expected int64
	cmd := &cobra.Command{Use: "supersede <task-id> <check-id>", Short: "Supersede an obsolete check without deleting history", Args: cobra.ExactArgs(2), RunE: func(cmd *cobra.Command, args []string) error {
		lease, err := cliTaskLease(leaseText)
		if err != nil {
			return err
		}
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.SupersedeCheck(context.Background(), args[0], token, cliTaskActor(actor), graphtask.SupersedeCheckInput{ExpectedRevision: expected, CheckID: args[1], Reason: reason, ReplacementText: replacement, ReplacementKind: replacementKind}, lease)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().Int64Var(&expected, "expected-revision", 0, "Current task revision fence")
	_ = cmd.MarkFlagRequired("expected-revision")
	cmd.Flags().StringVar(&reason, "reason", "", "Why the check is obsolete")
	_ = cmd.MarkFlagRequired("reason")
	cmd.Flags().StringVar(&replacement, "replacement", "", "Optional replacement check text")
	cmd.Flags().StringVar(&replacementKind, "replacement-kind", "", "Optional acceptance or test replacement kind")
	claimFlags(cmd, &token, &actor, &leaseText)
	return cmd
}

type taskRevisionPatch struct {
	Title                 *string   `json:"title,omitempty"`
	Description           *string   `json:"description,omitempty"`
	Type                  *string   `json:"type,omitempty"`
	Priority              *int      `json:"priority,omitempty"`
	ParentID              *string   `json:"parent_id,omitempty"`
	DependsOn             *[]string `json:"depends_on,omitempty"`
	AddAcceptanceCriteria []string  `json:"add_acceptance_criteria,omitempty"`
	AddTests              []string  `json:"add_tests,omitempty"`
}

func newTaskReviseCmd() *cobra.Command {
	var token, actor, reason, leaseText string
	var expected int64
	cmd := &cobra.Command{Use: "revise <task-id> <patch-file|->", Short: "Revise a claimed task specification", Args: cobra.ExactArgs(2), RunE: func(cmd *cobra.Command, args []string) error {
		var patch taskRevisionPatch
		if err := decodeStrictTaskJSON(cmd.InOrStdin(), args[1], &patch, "task revision patch"); err != nil {
			return err
		}
		lease, err := cliTaskLease(leaseText)
		if err != nil {
			return err
		}
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.Revise(context.Background(), args[0], token, cliTaskActor(actor), graphtask.ReviseInput{ExpectedRevision: expected, Reason: reason, Title: patch.Title, Description: patch.Description, Type: patch.Type, Priority: patch.Priority, ParentID: patch.ParentID, DependsOn: patch.DependsOn, AddAcceptanceCriteria: patch.AddAcceptanceCriteria, AddTests: patch.AddTests}, lease)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().Int64Var(&expected, "expected-revision", 0, "Current task revision fence")
	_ = cmd.MarkFlagRequired("expected-revision")
	cmd.Flags().StringVar(&reason, "reason", "", "Why the specification changed")
	_ = cmd.MarkFlagRequired("reason")
	claimFlags(cmd, &token, &actor, &leaseText)
	return cmd
}

func newTaskCommentCmd() *cobra.Command {
	var token, actor, kind, key, leaseText string
	cmd := &cobra.Command{Use: "comment <task-id> <body>", Short: "Append a typed durable task comment", Args: cobra.ExactArgs(2), RunE: func(cmd *cobra.Command, args []string) error {
		lease, err := cliTaskLease(leaseText)
		if err != nil {
			return err
		}
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.AddComment(context.Background(), args[0], token, cliTaskActor(actor), kind, args[1], key, lease)
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&kind, "kind", "note", "note, decision, problem, lesson, or knowledge")
	cmd.Flags().StringVar(&key, "idempotency-key", "", "Stable caller key")
	claimFlags(cmd, &token, &actor, &leaseText)
	return cmd
}

func newTaskDependencyCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "dependency", Aliases: []string{"dep"}, Short: "Manage blocking dependencies"}
	cmd.AddCommand(newTaskDependencyChangeCmd(true), newTaskDependencyChangeCmd(false))
	return cmd
}
func newTaskDependencyChangeCmd(add bool) *cobra.Command {
	verb := "add"
	if !add {
		verb = "remove"
	}
	var actor string
	label := "Add"
	if !add {
		label = "Remove"
	}
	cmd := &cobra.Command{Use: verb + " <task-id> <depends-on-id>", Short: label + " a blocking dependency", Args: cobra.ExactArgs(2), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		var v graphtask.Task
		if add {
			v, err = svc.AddDependency(context.Background(), args[0], args[1], cliTaskActor(actor))
		} else {
			v, err = svc.RemoveDependency(context.Background(), args[0], args[1], cliTaskActor(actor))
		}
		if err != nil {
			return err
		}
		return printTaskJSON(v)
	}}
	cmd.Flags().StringVar(&actor, "agent", "", "Agent identity")
	return cmd
}
