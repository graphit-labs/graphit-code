package sessioncontext

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

// StoreFunc applies the store-backed half of a hook event: the Task lifecycle effect
// when req.Task, and reading mandatory memory when req.Mandatory. The core applies it
// in-process; the hook binary asks the daemon.
type StoreFunc func(ctx context.Context, req sessionhook.StoreRequest) (sessionhook.StoreResult, error)

// Render applies a hook event for projectDir and renders the payload the host expects.
// store is called only when the event needs a store, and before anything is recorded,
// so a caller whose store is unavailable can hand the whole event elsewhere.
func Render(ctx context.Context, projectDir, format string, input []byte, store StoreFunc) ([]byte, error) {
	if ctx == nil {
		ctx = context.Background()
	}
	modules := ModuleContext(projectDir)
	marker := ""
	if strings.EqualFold(format, sessionhook.FormatSessionPrompt) {
		marker = sessionBootstrapMarker(projectDir, input)
	}
	bootstrapDelivered := markerExists(marker)
	includeMandatory := NeedsMandatory(format, input) && !bootstrapDelivered

	req := sessionhook.StoreRequest{
		Format: format, ProjectDir: projectDir, Input: input,
		Task:      projectDir != "" && !modules.TaskDisabled,
		Mandatory: projectDir != "" && includeMandatory && !modules.MemoryDisabled,
	}
	var stored sessionhook.StoreResult
	if req.Task || req.Mandatory {
		var err error
		if stored, err = store(ctx, req); err != nil {
			return nil, err
		}
	}
	if marker != "" && !bootstrapDelivered {
		writeMarker(marker)
	}

	context := modules
	if includeMandatory || strings.EqualFold(format, sessionhook.FormatToolContext) {
		context = Build(projectDir)
	}
	if req.Mandatory {
		context.Mandatory, context.MandatoryLoaded = stored.Mandatory, stored.MandatoryLoaded
	}
	context.BootstrapDelivered = bootstrapDelivered
	return sessionhook.RenderWithContext(format, input, context)
}

// NeedsMandatory reports whether the event is a boundary that carries mandatory memory.
func NeedsMandatory(format string, input []byte) bool {
	switch strings.ToLower(format) {
	case sessionhook.FormatFirstInvocation:
		var event struct {
			InvocationNum *int `json:"invocationNum"`
		}
		return json.Unmarshal(input, &event) == nil && event.InvocationNum != nil && *event.InvocationNum == 0
	case sessionhook.FormatSessionStart, sessionhook.FormatAdditionalContext,
		sessionhook.FormatPlainContext, sessionhook.FormatSubagentStart,
		sessionhook.FormatCursorSubagentTask, sessionhook.FormatSessionPrompt:
		return true
	default:
		return false
	}
}

// sessionBootstrapMarker is the per-session marker file, or "" when the session cannot
// be identified.
func sessionBootstrapMarker(projectDir string, input []byte) string {
	if projectDir == "" {
		return ""
	}
	var event struct {
		SessionID string `json:"session_id"`
	}
	if json.Unmarshal(input, &event) != nil {
		return ""
	}
	// The identifier reaches us from the host, so it never becomes a path
	// element itself; the digest keeps a hostile or exotic value contained.
	id := strings.TrimSpace(event.SessionID)
	if id == "" {
		return ""
	}
	return brand.ProjectRuntimePath(projectDir, "cache", "session-bootstrap", fmt.Sprintf("%x", sha256.Sum256([]byte(id))))
}

func markerExists(marker string) bool {
	if marker == "" {
		return false
	}
	_, err := os.Stat(marker)
	return err == nil
}

func writeMarker(marker string) {
	if err := os.MkdirAll(filepath.Dir(marker), 0o755); err != nil {
		return
	}
	_ = os.WriteFile(marker, nil, 0o644)
}
