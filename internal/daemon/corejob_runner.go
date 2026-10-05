package daemon

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/clirun"
	"github.com/graphit-labs/graphit-code/internal/dream"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/projectversion"
	"github.com/graphit-labs/graphit-code/internal/storework"
)

// Only daemon-owned core jobs use this dispatcher. It is not a CLI interpreter:
// each supported invocation maps to an explicit service, with no process globals.
func jobRunner(ctx context.Context, args []string) clirun.Runner {
	if len(args) > 0 {
		switch args[0] {
		case "sync", "ast", "wiki", "knowledge", "hub", "dream":
			return coreRunner
		case "memory", "task":
			// The thin store binary cannot register Hub's scoped authorizer and
			// delegates Broker work to core. Execute that exceptional work here.
			if snapshot, err := auth.ResolveActive(ctx); err == nil && auth.UsesScopedS3(snapshot.Provider) {
				return coreRunner
			}
		}
	}
	return cliRunner
}

type inProcessRunner struct{}

func (inProcessRunner) Run(ctx context.Context, spec clirun.Spec) (err error) {
	defer func() {
		if p := recover(); p != nil {
			err = fmt.Errorf("daemon job %v panicked: %v", spec.Args, p)
		}
	}()
	if err := ctx.Err(); err != nil {
		return err
	}
	if spec.Dir == "" || !filepath.IsAbs(spec.Dir) {
		return fmt.Errorf("daemon job requires an absolute project directory")
	}
	if len(spec.Args) == 0 {
		return fmt.Errorf("daemon job requires an operation")
	}
	version := ""
	for _, env := range spec.Env {
		if v, ok := strings.CutPrefix(env, projectversion.EnvVersion()+"="); ok {
			version = v
		}
	}
	if version == "" {
		info, e := projectversion.Resolve(spec.Dir, "")
		if e != nil {
			return e
		}
		version = info.Version
	}
	spec.Env = []string{projectversion.EnvVersion() + "=" + version}
	w := spec.Stderr
	if w == nil {
		w = io.Discard
	}
	logger := slog.New(slog.NewTextHandler(w, nil))
	a := spec.Args
	// These orchestrators scope individual phases. An outer lease would be
	// reentrant in sync and would deadlock Dream's same-project MCP callbacks.
	if len(a) == 1 && a[0] == "sync" {
		return syncProjectJobs(ctx, spec, logger)
	}
	if len(a) >= 4 && a[0] == "dream" && a[1] == "run" && a[2] == "--session" {
		agent := ""
		if len(a) == 6 && a[4] == "--agent" {
			agent = a[5]
		} else if len(a) != 4 {
			return fmt.Errorf("unsupported daemon Dream arguments: %v", a)
		}
		return dream.ExecuteSessionVersion(ctx, spec.Dir, version, agent, a[3], func(format string, args ...any) { logger.Info(fmt.Sprintf(format, args...)) })
	}
	release, err := projectversion.AcquireContext(ctx, spec.Dir, version)
	if err != nil {
		return err
	}
	defer release()
	ctx = storework.WithOutputWriter(ctx, w)
	switch {
	case len(a) >= 2 && a[0] == "hub" && a[1] == "hydrate":
		if len(a) != 2 && (len(a) != 3 || a[2] != "--json") {
			break
		}
		r, err := hub.HydrateProjectLanceWithResult(ctx, spec.Dir, loadProjectConfigFromDir(spec.Dir))
		if err != nil {
			return err
		}
		if len(a) == 3 && spec.Stdout != nil {
			return json.NewEncoder(spec.Stdout).Encode(hydrationReport{r.ASTBaseChanged, r.KnowledgeBaseChanged})
		}
		return nil
	case len(a) >= 2 && a[0] == "ast" && a[1] == "index":
		var changes *astChangeSet
		if len(a) == 4 && a[2] == "--changes-from" && a[3] == "-" && spec.Stdin != nil {
			changes = &astChangeSet{}
			if err := json.NewDecoder(spec.Stdin).Decode(changes); err != nil {
				return err
			}
		} else if len(a) != 2 {
			break
		}
		return indexProjectAST(ctx, spec.Dir, changes, logger)
	case len(a) >= 2 && a[0] == "ast" && a[1] == "embed":
		if len(a) != 2 && (len(a) != 3 || a[2] != "--if-idle") {
			break
		}
		return embedProjectAST(ctx, spec.Dir, logger)
	case len(a) == 2 && a[0] == "wiki" && a[1] == "embed":
		return embedProjectWiki(ctx, spec.Dir)
	case len(a) == 2 && a[0] == "knowledge" && a[1] == "index":
		return indexProjectKnowledge(ctx, spec.Dir)
	case len(a) >= 2 && a[0] == "memory":
		user := len(a) == 3 && a[2] == "--user"
		if len(a) != 2 && !user {
			break
		}
		switch a[1] {
		case "index":
			return storework.IndexMemory(ctx, spec.Dir, user, false)
		case "maintain":
			return storework.MaintainMemory(ctx, spec.Dir, user)
		case "embed":
			return storework.EmbedMemory(ctx, spec.Dir, user)
		}
	case len(a) == 2 && a[0] == "task" && a[1] == "maintain":
		return storework.MaintainTasks(ctx, spec.Dir)
	}
	return fmt.Errorf("unsupported daemon operation: %v", a)
}
