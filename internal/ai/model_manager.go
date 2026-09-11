package ai

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// ProgressFunc reports one artifact download. file is the bundle-relative path.
type ProgressFunc func(file string, downloaded, total int64)

// ResolvedModel is a validated manifest anchored to its global bundle directory.
// It is the only local-model artifact resolution path.
type ResolvedModel struct {
	Manifest ModelManifest
	Dir      string

	Logger *slog.Logger

	OnProgress ProgressFunc
}

func (m *ResolvedModel) log() *slog.Logger { return slogutil.Resolve(m.Logger) }

func ModelsDir() (string, error) {
	if d := os.Getenv(brand.EnvVar("MODEL_CACHE")); d != "" {
		return filepath.Abs(d)
	}
	global := brand.GlobalDir()
	if global == "" {
		return "", fmt.Errorf("home dir: cannot resolve the global %s directory", brand.Brand)
	}
	return filepath.Join(global, "models"), nil
}

func (m *ResolvedModel) ArtifactPath(role string) (string, error) {
	artifact, ok := m.Manifest.artifact(role)
	if !ok {
		return "", fmt.Errorf("model %q has no artifact role %q", m.Manifest.ID, role)
	}
	return safeBundlePath(m.Dir, artifact.Path)
}

func (m *ResolvedModel) EntrypointPath(name string) (string, error) {
	role, ok := m.Manifest.Runtime.Entrypoints[name]
	if !ok {
		return "", fmt.Errorf("model %q has no runtime entrypoint %q", m.Manifest.ID, name)
	}
	return m.ArtifactPath(role)
}

func (m *ResolvedModel) Present() bool {
	for _, artifact := range m.Manifest.Artifacts {
		if !artifact.required() {
			continue
		}
		path, err := safeBundlePath(m.Dir, artifact.Path)
		if err != nil || validateArtifactFile(path, artifact) != nil {
			return false
		}
	}
	return true
}

// Ensure resolves every required artifact. Setup and on_demand bundles may fetch
// missing artifacts; never bundles only validate files already supplied.
func (m *ResolvedModel) Ensure(ctx context.Context) (map[string]string, error) {
	paths := make(map[string]string, len(m.Manifest.Artifacts))
	for _, artifact := range m.Manifest.Artifacts {
		path, err := safeBundlePath(m.Dir, artifact.Path)
		if err != nil {
			return nil, fmt.Errorf("model %q artifact %q: %w", m.Manifest.ID, artifact.Role, err)
		}
		validationErr := validateArtifactFile(path, artifact)
		if validationErr == nil {
			paths[artifact.Role] = path
			continue
		}
		if !artifact.required() && (len(artifact.Sources) == 0 || m.Manifest.FetchPolicy == FetchPolicyNever) {
			continue
		}
		if m.Manifest.FetchPolicy == FetchPolicyNever {
			return nil, fmt.Errorf("model %q artifact %q is not ready at %s and fetch_policy is never: %w", m.Manifest.ID, artifact.Role, path, validationErr)
		}
		if len(artifact.Sources) == 0 {
			return nil, fmt.Errorf("model %q artifact %q is not ready at %s and declares no download source: %w", m.Manifest.ID, artifact.Role, path, validationErr)
		}
		if err := m.fetchArtifact(ctx, artifact, path); err != nil {
			if !artifact.required() {
				m.log().Warn("optional model artifact is unavailable", "model", m.Manifest.ID, "artifact", artifact.Role, "error", err)
				continue
			}
			return nil, fmt.Errorf("resolve model %q artifact %q: %w", m.Manifest.ID, artifact.Role, err)
		}
		paths[artifact.Role] = path
	}
	return paths, nil
}

func (m *ResolvedModel) fetchArtifact(ctx context.Context, artifact ModelArtifact, destination string) error {
	if err := os.MkdirAll(filepath.Dir(destination), 0o755); err != nil {
		return fmt.Errorf("create artifact directory: %w", err)
	}
	var attempts []error
	for _, source := range artifact.Sources {
		if err := m.downloadArtifact(ctx, source.URL, destination, artifact); err != nil {
			attempts = append(attempts, fmt.Errorf("%s: %w", source.URL, err))
			continue
		}
		return nil
	}
	return errors.Join(attempts...)
}

func (m *ResolvedModel) downloadArtifact(ctx context.Context, sourceURL, destination string, artifact ModelArtifact) error {
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, sourceURL, nil)
	if err != nil {
		return err
	}
	client := &http.Client{CheckRedirect: func(request *http.Request, via []*http.Request) error {
		if len(via) >= 10 {
			return fmt.Errorf("too many redirects")
		}
		return validateModelSourceURL(request.URL.String())
	}}
	response, err := client.Do(request)
	if err != nil {
		return err
	}
	defer func() { _ = response.Body.Close() }()
	if response.StatusCode != http.StatusOK {
		return fmt.Errorf("HTTP %d", response.StatusCode)
	}

	temporary, err := os.CreateTemp(filepath.Dir(destination), "."+filepath.Base(destination)+"-*.tmp")
	if err != nil {
		return err
	}
	temporaryPath := temporary.Name()
	committed := false
	defer func() {
		_ = temporary.Close()
		if !committed {
			_ = os.Remove(temporaryPath)
		}
	}()

	var writer io.Writer = temporary
	if m.OnProgress != nil {
		m.OnProgress(artifact.Path, 0, response.ContentLength)
		writer = &progressWriter{w: temporary, file: artifact.Path, total: response.ContentLength, report: m.OnProgress}
	}
	written, err := io.Copy(writer, response.Body)
	if closeErr := temporary.Close(); err == nil {
		err = closeErr
	}
	if err != nil {
		return fmt.Errorf("write temporary artifact: %w", err)
	}
	if response.ContentLength > 0 && written != response.ContentLength {
		return fmt.Errorf("incomplete download: wrote %d of %d bytes", written, response.ContentLength)
	}
	if err := validateArtifactFile(temporaryPath, artifact); err != nil {
		return fmt.Errorf("downloaded artifact is invalid: %w", err)
	}
	if err := os.Rename(temporaryPath, destination); err != nil {
		if removeErr := os.Remove(destination); removeErr != nil && !os.IsNotExist(removeErr) {
			return fmt.Errorf("replace invalid artifact: %w", removeErr)
		}
		if err := os.Rename(temporaryPath, destination); err != nil {
			return err
		}
	}
	committed = true
	return nil
}

func validateArtifactFile(path string, artifact ModelArtifact) error {
	info, err := os.Stat(path)
	if err != nil {
		return err
	}
	if !info.Mode().IsRegular() {
		return fmt.Errorf("%s is not a regular file", path)
	}
	if info.Size() < artifact.MinSize {
		return fmt.Errorf("%s is %d bytes, expected at least %d", path, info.Size(), artifact.MinSize)
	}
	if artifact.SHA256 == "" {
		return nil
	}
	f, err := os.Open(path)
	if err != nil {
		return err
	}
	hash := sha256.New()
	_, copyErr := io.Copy(hash, f)
	closeErr := f.Close()
	if copyErr != nil {
		return copyErr
	}
	if closeErr != nil {
		return closeErr
	}
	actual := hex.EncodeToString(hash.Sum(nil))
	if !strings.EqualFold(actual, artifact.SHA256) {
		return fmt.Errorf("%s sha256 is %s, expected %s", path, actual, artifact.SHA256)
	}
	return nil
}

func safeBundlePath(root, relative string) (string, error) {
	clean, err := cleanArtifactPath(relative)
	if err != nil {
		return "", err
	}
	rootAbs, err := filepath.Abs(root)
	if err != nil {
		return "", err
	}
	joined := filepath.Join(rootAbs, clean)
	rel, err := filepath.Rel(rootAbs, joined)
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("artifact path %q escapes the model bundle", relative)
	}
	if err := rejectSymlinkComponents(rootAbs, clean); err != nil {
		return "", err
	}
	return joined, nil
}

func cleanArtifactPath(relative string) (string, error) {
	if strings.TrimSpace(relative) == "" {
		return "", fmt.Errorf("artifact path is required")
	}
	if filepath.IsAbs(relative) {
		return "", fmt.Errorf("artifact path %q must be relative to the model bundle", relative)
	}
	clean := filepath.Clean(filepath.FromSlash(relative))
	if clean == "." || clean == ".." || strings.HasPrefix(clean, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("artifact path %q escapes the model bundle", relative)
	}
	return clean, nil
}

func rejectSymlinkComponents(root, relative string) error {
	current := root
	for _, component := range strings.Split(relative, string(filepath.Separator)) {
		current = filepath.Join(current, component)
		info, err := os.Lstat(current)
		if os.IsNotExist(err) {
			return nil
		}
		if err != nil {
			return err
		}
		if info.Mode()&os.ModeSymlink != 0 {
			return fmt.Errorf("artifact path component %s is a symlink", current)
		}
	}
	return nil
}

// PrefetchConfiguredLocalModels resolves setup-policy bundles for services that
// currently use their local backend. Failures are returned to explicit callers;
// the daemon invokes this as best-effort background warmup.
func PrefetchConfiguredLocalModels(ctx context.Context) error {
	var errs []error
	for _, task := range []ModelTask{ModelTaskEmbedding, ModelTaskRerank} {
		if !ConfiguredServiceIsLocal(task) {
			continue
		}
		model, err := LoadConfiguredModel(task)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		if model.Manifest.FetchPolicy != FetchPolicySetup {
			continue
		}
		if _, err := model.Ensure(ctx); err != nil {
			errs = append(errs, err)
		}
	}
	return errors.Join(errs...)
}

// ConfiguredServiceIsLocal reports whether the active provider routes this service to local ONNX.
func ConfiguredServiceIsLocal(task ModelTask) bool {
	snapshot, ok := activeAuthSnapshot()
	if !ok {
		return false
	}
	mode := snapshot.Provider.AI.Embedding.Mode
	if task == ModelTaskRerank {
		mode = snapshot.Provider.AI.Rerank.Mode
	}
	return mode == "" || mode == "local"
}

type progressWriter struct {
	w          io.Writer
	file       string
	total      int64
	downloaded int64
	report     ProgressFunc
}

func (writer *progressWriter) Write(data []byte) (int, error) {
	n, err := writer.w.Write(data)
	writer.downloaded += int64(n)
	writer.report(writer.file, writer.downloaded, writer.total)
	return n, err
}
