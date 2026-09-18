package commands

import (
	"encoding/json"
	"errors"
	"strings"

	graphtask "github.com/graphit-labs/graphit-code/internal/task"
	"github.com/spf13/cobra"
)

func newTaskSessionCmd() *cobra.Command {
	cmd := &cobra.Command{Use: "session", Short: "Preserve, coordinate and resume durable user requests", Long: "A Task session preserves the complete request, strategy, checkpoints and associated tasks across agents. It closes only through explicit completion or cancellation after its tasks are terminal."}
	cmd.AddCommand(newTaskSessionCreateCmd(), newTaskSessionGetCmd(), newTaskSessionListCmd(), newTaskSessionSearchCmd(), newTaskSessionClaimCmd(), newTaskSessionReviseCmd(), newTaskSessionCheckpointCmd(), newTaskSessionHeartbeatCmd(), newTaskSessionReleaseCmd(), newTaskSessionCompleteCmd(), newTaskSessionCancelCmd(), newTaskSessionForceTakeoverCmd())
	return cmd
}

func printTaskSessionJSON(cmd *cobra.Command, value any) error {
	encoder := json.NewEncoder(cmd.OutOrStdout())
	encoder.SetIndent("", "  ")
	return encoder.Encode(value)
}

func newTaskSessionCreateCmd() *cobra.Command {
	var description, strategy, key, actor string
	cmd := &cobra.Command{Use: "create <title>", Short: "Describe a new durable request and its strategy", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.SessionCreate(cmd.Context(), graphtask.SessionCreateInput{Title: args[0], Description: description, Strategy: strategy, IdempotencyKey: key, Actor: cliTaskActor(actor)})
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
	cmd.Flags().StringVar(&description, "description", "", "Complete current user request, scope, requirements and success criteria")
	_ = cmd.MarkFlagRequired("description")
	cmd.Flags().StringVar(&strategy, "strategy", "", "Approach, decomposition, uncertainties and validation strategy")
	_ = cmd.MarkFlagRequired("strategy")
	cmd.Flags().StringVar(&key, "idempotency-key", "", "Stable key for this logical request")
	cmd.Flags().StringVar(&actor, "agent", "", "Coordinator identity (defaults to this Graphit unit)")
	return cmd
}

func newTaskSessionGetCmd() *cobra.Command {
	return &cobra.Command{Use: "get <id>", Aliases: []string{"show"}, Short: "Read request, checkpoint history, revisions and associated task summaries", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.SessionGet(cmd.Context(), args[0])
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
}

func newTaskSessionListCmd() *cobra.Command {
	var status, owner string
	var active bool
	cmd := &cobra.Command{Use: "list", Short: "Find sessions, including unfinished requests to resume", Args: cobra.NoArgs, RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.SessionList(cmd.Context(), graphtask.SessionListOptions{Status: status, Owner: owner, Active: active})
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
	cmd.Flags().StringVar(&status, "status", "", "open, in_progress, completed, or cancelled")
	cmd.Flags().StringVar(&owner, "owner", "", "Exact coordinator identity")
	cmd.Flags().BoolVar(&active, "active", false, "Only nonterminal sessions")
	return cmd
}

func newTaskSessionSearchCmd() *cobra.Command {
	var limit int
	cmd := &cobra.Command{Use: "search <query>", Short: "Search requests, strategies and durable checkpoint history", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.SessionSearch(cmd.Context(), args[0], limit)
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
	cmd.Flags().IntVar(&limit, "limit", 20, "Maximum ranked results")
	return cmd
}

func newTaskSessionClaimCmd() *cobra.Command {
	var actor, leaseText string
	cmd := &cobra.Command{Use: "claim <id>", Short: "Claim session coordination independently of task work", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		lease, err := cliTaskLease(leaseText)
		if err != nil {
			return err
		}
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.SessionClaim(cmd.Context(), args[0], cliTaskActor(actor), lease)
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
	cmd.Flags().StringVar(&actor, "agent", "", "Coordinator identity (defaults to this Graphit unit)")
	cmd.Flags().StringVar(&leaseText, "lease", "1h", "Positive coordinator lease")
	return cmd
}

type taskSessionRevisionPatch struct {
	Title       *string `json:"title,omitempty"`
	Description *string `json:"description,omitempty"`
	Strategy    *string `json:"strategy,omitempty"`
}

func newTaskSessionReviseCmd() *cobra.Command {
	var actor, token, reason, leaseText string
	var expected int64
	cmd := &cobra.Command{Use: "revise <id> <patch-file|->", Short: "Revise request or strategy while preserving prior specifications", Args: cobra.ExactArgs(2), RunE: func(cmd *cobra.Command, args []string) error {
		var patch taskSessionRevisionPatch
		if err := decodeStrictTaskJSON(cmd.InOrStdin(), args[1], &patch, "session revision patch"); err != nil {
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
		v, err := svc.SessionRevise(cmd.Context(), args[0], token, cliTaskActor(actor), graphtask.SessionReviseInput{ExpectedRevision: expected, Reason: reason, Title: patch.Title, Description: patch.Description, Strategy: patch.Strategy}, lease)
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
	cmd.Flags().Int64Var(&expected, "expected-revision", 0, "Current session revision fence")
	_ = cmd.MarkFlagRequired("expected-revision")
	cmd.Flags().StringVar(&reason, "reason", "", "What changed and its effect on the request and task plan")
	_ = cmd.MarkFlagRequired("reason")
	claimFlags(cmd, &token, &actor, &leaseText)
	return cmd
}

func newTaskSessionCheckpointCmd() *cobra.Command {
	var actor, token, leaseText string
	cmd := &cobra.Command{Use: "checkpoint <id> <file|->", Short: "Append descriptive progress, problems, decisions and exact continuation from JSON", Args: cobra.ExactArgs(2), RunE: func(cmd *cobra.Command, args []string) error {
		var input graphtask.SessionCheckpointInput
		if err := decodeStrictTaskJSON(cmd.InOrStdin(), args[1], &input, "session checkpoint"); err != nil {
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
		v, err := svc.SessionCheckpoint(cmd.Context(), args[0], token, cliTaskActor(actor), input, lease)
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
	claimFlags(cmd, &token, &actor, &leaseText)
	return cmd
}

func newTaskSessionHeartbeatCmd() *cobra.Command {
	var actor, token, leaseText string
	cmd := &cobra.Command{Use: "heartbeat <id>", Short: "Renew coordination without recording fictional progress", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		lease, err := cliTaskLease(leaseText)
		if err != nil {
			return err
		}
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.SessionHeartbeat(cmd.Context(), args[0], token, cliTaskActor(actor), lease)
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
	claimFlags(cmd, &token, &actor, &leaseText)
	return cmd
}

func newTaskSessionReleaseCmd() *cobra.Command {
	var actor, token, summary, next string
	cmd := &cobra.Command{Use: "release <id>", Short: "Hand off coordination with current state and exact continuation", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.SessionRelease(cmd.Context(), args[0], token, cliTaskActor(actor), summary, next)
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
	cmd.Flags().StringVar(&summary, "summary", "", "Results, remaining work, blockers and relevant task references")
	_ = cmd.MarkFlagRequired("summary")
	cmd.Flags().StringVar(&next, "next-step", "", "Exact continuation action with target and completion condition")
	_ = cmd.MarkFlagRequired("next-step")
	claimFlags(cmd, &token, &actor, nil)
	return cmd
}

func newTaskSessionCompleteCmd() *cobra.Command {
	var actor, token, summary string
	cmd := &cobra.Command{Use: "complete <id>", Short: "Explicitly close a fulfilled request after associated tasks are terminal", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.SessionComplete(cmd.Context(), args[0], token, cliTaskActor(actor), summary)
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
	cmd.Flags().StringVar(&summary, "summary", "", "Final outcomes, evidence, cancelled scope and residual limitations")
	_ = cmd.MarkFlagRequired("summary")
	claimFlags(cmd, &token, &actor, nil)
	return cmd
}

func newTaskSessionCancelCmd() *cobra.Command {
	var actor, token, reason string
	cmd := &cobra.Command{Use: "cancel <id>", Short: "Cancel an obsolete request after explicitly resolving its tasks", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.SessionCancel(cmd.Context(), args[0], token, cliTaskActor(actor), reason)
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
	cmd.Flags().StringVar(&reason, "reason", "", "Cancellation rationale, consequences and replacement if any")
	_ = cmd.MarkFlagRequired("reason")
	claimFlags(cmd, &token, &actor, nil)
	return cmd
}

func newTaskSessionForceTakeoverCmd() *cobra.Command {
	var actor, confirm, reason, leaseText string
	var expected int64
	cmd := &cobra.Command{Use: "force-takeover <id>", Short: "Recover coordination from an unrecoverable owner with explicit fencing", Args: cobra.ExactArgs(1), RunE: func(cmd *cobra.Command, args []string) error {
		if strings.TrimSpace(leaseText) == "" {
			return errors.New("session takeover requires an explicit positive replacement lease")
		}
		lease, err := cliTaskLease(leaseText)
		if err != nil {
			return err
		}
		svc, err := currentTaskService()
		if err != nil {
			return err
		}
		v, err := svc.SessionForceTakeover(cmd.Context(), args[0], cliTaskActor(actor), graphtask.ForceTakeoverInput{ExpectedRevision: expected, ConfirmID: confirm, Reason: reason}, lease)
		if err != nil {
			return err
		}
		return printTaskSessionJSON(cmd, v)
	}}
	cmd.Flags().StringVar(&actor, "agent", "", "Different new coordinator identity")
	cmd.Flags().StringVar(&confirm, "confirm-id", "", "Exact session ID confirmation")
	_ = cmd.MarkFlagRequired("confirm-id")
	cmd.Flags().Int64Var(&expected, "expected-revision", 0, "Current session revision fence")
	_ = cmd.MarkFlagRequired("expected-revision")
	cmd.Flags().StringVar(&reason, "reason", "", "Evidence the current coordinator cannot continue")
	_ = cmd.MarkFlagRequired("reason")
	cmd.Flags().StringVar(&leaseText, "lease", "", "Positive replacement coordinator lease")
	_ = cmd.MarkFlagRequired("lease")
	return cmd
}
