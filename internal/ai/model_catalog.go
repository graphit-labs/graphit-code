package ai

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"slices"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
)

const (
	ModelManifestVersion = 1
	modelManifestFile    = "manifest.json"

	EmbeddingModelConfigKey = "models.embedding.id"
	RerankModelConfigKey    = "models.rerank.id"

	DefaultEmbeddingModelID = "coderankembed"
	DefaultRerankModelID    = "bge-reranker-base"
)

type ModelTask string

const (
	ModelTaskEmbedding ModelTask = "embedding"
	ModelTaskRerank    ModelTask = "rerank"
	ModelTaskGenerate  ModelTask = "generate"
)

type FetchPolicy string

const (
	FetchPolicySetup    FetchPolicy = "setup"
	FetchPolicyOnDemand FetchPolicy = "on_demand"
	FetchPolicyNever    FetchPolicy = "never"
)

type ModelManifest struct {
	SchemaVersion int                  `json:"schema_version"`
	ID            string               `json:"id"`
	Name          string               `json:"name,omitempty"`
	Revision      string               `json:"revision,omitempty"`
	Task          ModelTask            `json:"task"`
	Runtime       ModelRuntimeConfig   `json:"runtime"`
	FetchPolicy   FetchPolicy          `json:"fetch_policy"`
	Artifacts     []ModelArtifact      `json:"artifacts"`
	Tokenizer     ModelTokenizerConfig `json:"tokenizer"`
	Text          ModelTextConfig      `json:"text,omitempty"`
	Inference     ModelInferenceConfig `json:"inference,omitempty"`
}

type ModelRuntimeConfig struct {
	Engine      string            `json:"engine"`
	Entrypoints map[string]string `json:"entrypoints,omitempty"`
}

type ModelArtifact struct {
	Role     string                `json:"role"`
	Path     string                `json:"path"`
	Format   string                `json:"format,omitempty"`
	Sources  []ModelArtifactSource `json:"sources,omitempty"`
	SHA256   string                `json:"sha256,omitempty"`
	MinSize  int64                 `json:"min_size,omitempty"`
	Required *bool                 `json:"required,omitempty"`
}

type ModelArtifactSource struct {
	URL string `json:"url"`
}

type ModelTokenizerConfig struct {
	Artifact string `json:"artifact"`
	Format   string `json:"format"`
}

type ModelTextConfig struct {
	MaxTokens      int    `json:"max_tokens,omitempty"`
	QueryPrefix    string `json:"query_prefix,omitempty"`
	DocumentPrefix string `json:"document_prefix,omitempty"`
	Truncation     string `json:"truncation,omitempty"`
	Padding        string `json:"padding,omitempty"`
}

type ModelInferenceConfig struct {
	Inputs         map[string]string `json:"inputs,omitempty"`
	Output         string            `json:"output,omitempty"`
	Pooling        string            `json:"pooling,omitempty"`
	Normalize      *bool             `json:"normalize,omitempty"`
	Dimensions     int               `json:"dimensions,omitempty"`
	MaxBatch       int               `json:"max_batch,omitempty"`
	ScoreTransform string            `json:"score_transform,omitempty"`
	PositiveClass  *int              `json:"positive_class,omitempty"`
}

var modelIDPattern = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._-]*$`)

var builtinModelManifests = map[string]ModelManifest{
	DefaultEmbeddingModelID: {
		SchemaVersion: ModelManifestVersion,
		ID:            DefaultEmbeddingModelID,
		Name:          "CodeRankEmbed-137M-INT8",
		Revision:      "builtin-1",
		Task:          ModelTaskEmbedding,
		Runtime:       ModelRuntimeConfig{Engine: "onnx", Entrypoints: map[string]string{"model": "model"}},
		FetchPolicy:   FetchPolicyOnDemand,
		Artifacts: []ModelArtifact{
			{Role: "model", Path: "model.onnx", Sources: []ModelArtifactSource{{URL: "https://huggingface.co/mrsladoje/CodeRankEmbed-onnx-int8/resolve/main/onnx/model.onnx"}}, MinSize: 100_000_000},
			{Role: "tokenizer", Path: "tokenizer.json", Format: "huggingface-json", Sources: []ModelArtifactSource{{URL: "https://huggingface.co/mrsladoje/CodeRankEmbed-onnx-int8/resolve/main/tokenizer.json"}}, MinSize: 500_000},
		},
		Tokenizer: ModelTokenizerConfig{Artifact: "tokenizer", Format: "huggingface-json"},
		Text: ModelTextConfig{
			MaxTokens:   512,
			QueryPrefix: "Represent this query for searching relevant code: ",
			Truncation:  "longest-first",
			Padding:     "longest",
		},
		Inference: ModelInferenceConfig{
			Inputs:     map[string]string{"input_ids": "input_ids", "attention_mask": "attention_mask"},
			Output:     "sentence_embedding",
			Pooling:    "none",
			Normalize:  boolPointer(true),
			Dimensions: EmbeddingDimensions,
		},
	},
	DefaultRerankModelID: {
		SchemaVersion: ModelManifestVersion,
		ID:            DefaultRerankModelID,
		Name:          DefaultRerankModelID,
		Revision:      "builtin-1",
		Task:          ModelTaskRerank,
		Runtime:       ModelRuntimeConfig{Engine: "onnx", Entrypoints: map[string]string{"model": "model"}},
		FetchPolicy:   FetchPolicyOnDemand,
		Artifacts: []ModelArtifact{
			{Role: "model", Path: "model.onnx", Sources: []ModelArtifactSource{{URL: "https://huggingface.co/BAAI/bge-reranker-base/resolve/main/onnx/model.onnx"}}, MinSize: 900_000_000},
			{Role: "tokenizer", Path: "tokenizer.json", Format: "huggingface-json", Sources: []ModelArtifactSource{{URL: "https://huggingface.co/BAAI/bge-reranker-base/resolve/main/tokenizer.json"}}, MinSize: 5_000_000},
		},
		Tokenizer: ModelTokenizerConfig{Artifact: "tokenizer", Format: "huggingface-json"},
		Text: ModelTextConfig{
			MaxTokens:  512,
			Truncation: "longest-first",
			Padding:    "longest",
		},
		Inference: ModelInferenceConfig{
			MaxBatch:       16,
			ScoreTransform: "auto",
		},
	},
}

func boolPointer(v bool) *bool { return &v }

func configuredModelID(task ModelTask) string {
	var key, fallback string
	switch task {
	case ModelTaskEmbedding:
		key, fallback = EmbeddingModelConfigKey, DefaultEmbeddingModelID
	case ModelTaskRerank:
		key, fallback = RerankModelConfigKey, DefaultRerankModelID
	default:
		return ""
	}
	if selected := strings.TrimSpace(config.ResolveConfig(key, nil, nil)); selected != "" {
		return selected
	}
	return fallback
}

func LoadConfiguredModel(task ModelTask) (*ResolvedModel, error) {
	return LoadModel(task, configuredModelID(task))
}

func LoadModel(task ModelTask, id string) (*ResolvedModel, error) {
	id = strings.TrimSpace(id)
	if !modelIDPattern.MatchString(id) {
		return nil, fmt.Errorf("invalid %s model id %q", task, id)
	}
	root, err := ModelsDir()
	if err != nil {
		return nil, err
	}

	manifest, builtin := builtinModelManifests[id]
	if builtin {
		manifest = cloneModelManifest(manifest)
	} else {
		manifestPath := filepath.Join(root, id, modelManifestFile)
		f, openErr := os.Open(manifestPath)
		if openErr != nil {
			return nil, fmt.Errorf("open model manifest %s: %w", manifestPath, openErr)
		}
		decoder := json.NewDecoder(f)
		decoder.DisallowUnknownFields()
		decodeErr := decoder.Decode(&manifest)
		if decodeErr == nil {
			var trailing any
			if trailingErr := decoder.Decode(&trailing); trailingErr != io.EOF {
				if trailingErr == nil {
					decodeErr = fmt.Errorf("multiple JSON values")
				} else {
					decodeErr = trailingErr
				}
			}
		}
		closeErr := f.Close()
		if decodeErr != nil {
			return nil, fmt.Errorf("parse model manifest %s: %w", manifestPath, decodeErr)
		}
		if closeErr != nil {
			return nil, fmt.Errorf("close model manifest %s: %w", manifestPath, closeErr)
		}
	}

	normalizeModelManifest(&manifest)
	if err := validateModelManifest(manifest, task, id, builtin); err != nil {
		return nil, err
	}
	return &ResolvedModel{Manifest: manifest, Dir: filepath.Join(root, id)}, nil
}

func cloneModelManifest(manifest ModelManifest) ModelManifest {
	data, _ := json.Marshal(manifest)
	var cloned ModelManifest
	_ = json.Unmarshal(data, &cloned)
	return cloned
}

func normalizeModelManifest(manifest *ModelManifest) {
	manifest.ID = strings.TrimSpace(manifest.ID)
	manifest.Name = strings.TrimSpace(manifest.Name)
	if manifest.Name == "" {
		manifest.Name = manifest.ID
	}
	manifest.Revision = strings.TrimSpace(manifest.Revision)
	if manifest.Revision == "" {
		manifest.Revision = "1"
	}
	manifest.Runtime.Engine = strings.ToLower(strings.TrimSpace(manifest.Runtime.Engine))
	if manifest.Runtime.Entrypoints == nil {
		manifest.Runtime.Entrypoints = map[string]string{"model": "model"}
	} else {
		entrypoints := make(map[string]string, len(manifest.Runtime.Entrypoints))
		for name, role := range manifest.Runtime.Entrypoints {
			entrypoints[strings.TrimSpace(name)] = strings.TrimSpace(role)
		}
		manifest.Runtime.Entrypoints = entrypoints
	}
	manifest.FetchPolicy = FetchPolicy(strings.ToLower(strings.TrimSpace(string(manifest.FetchPolicy))))
	if manifest.FetchPolicy == "" {
		manifest.FetchPolicy = FetchPolicyNever
	}
	manifest.Tokenizer.Artifact = strings.TrimSpace(manifest.Tokenizer.Artifact)
	if manifest.Tokenizer.Artifact == "" {
		manifest.Tokenizer.Artifact = "tokenizer"
	}
	manifest.Tokenizer.Format = strings.ToLower(strings.TrimSpace(manifest.Tokenizer.Format))
	if manifest.Text.MaxTokens == 0 {
		manifest.Text.MaxTokens = 512
	}
	manifest.Text.Truncation = strings.ToLower(strings.TrimSpace(manifest.Text.Truncation))
	if manifest.Text.Truncation == "" {
		manifest.Text.Truncation = "longest-first"
	}
	manifest.Text.Padding = strings.ToLower(strings.TrimSpace(manifest.Text.Padding))
	if manifest.Text.Padding == "" {
		manifest.Text.Padding = "longest"
	}
	if manifest.Inference.Inputs == nil {
		manifest.Inference.Inputs = map[string]string{}
	} else {
		inputs := make(map[string]string, len(manifest.Inference.Inputs))
		for role, name := range manifest.Inference.Inputs {
			inputs[strings.TrimSpace(role)] = strings.TrimSpace(name)
		}
		manifest.Inference.Inputs = inputs
	}
	manifest.Inference.Output = strings.TrimSpace(manifest.Inference.Output)
	manifest.Inference.Pooling = strings.ToLower(strings.TrimSpace(manifest.Inference.Pooling))
	if manifest.Inference.Pooling == "" {
		manifest.Inference.Pooling = "none"
	}
	manifest.Inference.ScoreTransform = strings.ToLower(strings.TrimSpace(manifest.Inference.ScoreTransform))
	if manifest.Inference.ScoreTransform == "" {
		manifest.Inference.ScoreTransform = "auto"
	}
	if manifest.Inference.MaxBatch == 0 {
		manifest.Inference.MaxBatch = 16
	}
	for i := range manifest.Artifacts {
		manifest.Artifacts[i].Role = strings.TrimSpace(manifest.Artifacts[i].Role)
		manifest.Artifacts[i].Path = filepath.ToSlash(strings.TrimSpace(manifest.Artifacts[i].Path))
		manifest.Artifacts[i].Format = strings.ToLower(strings.TrimSpace(manifest.Artifacts[i].Format))
		manifest.Artifacts[i].SHA256 = strings.ToLower(strings.TrimSpace(manifest.Artifacts[i].SHA256))
		for j := range manifest.Artifacts[i].Sources {
			manifest.Artifacts[i].Sources[j].URL = strings.TrimSpace(manifest.Artifacts[i].Sources[j].URL)
		}
	}
}

func validateModelManifest(manifest ModelManifest, task ModelTask, selectedID string, builtin bool) error {
	label := fmt.Sprintf("model manifest %q", selectedID)
	if manifest.SchemaVersion != ModelManifestVersion {
		return fmt.Errorf("%s has schema_version %d, want %d", label, manifest.SchemaVersion, ModelManifestVersion)
	}
	if manifest.ID != selectedID {
		return fmt.Errorf("%s declares id %q", label, manifest.ID)
	}
	if manifest.Task != task {
		return fmt.Errorf("%s is for task %q, not %q", label, manifest.Task, task)
	}
	if manifest.Runtime.Engine != "onnx" {
		return fmt.Errorf("%s uses unsupported runtime engine %q", label, manifest.Runtime.Engine)
	}
	switch manifest.FetchPolicy {
	case FetchPolicySetup, FetchPolicyOnDemand, FetchPolicyNever:
	default:
		return fmt.Errorf("%s has unsupported fetch_policy %q", label, manifest.FetchPolicy)
	}
	if len(manifest.Artifacts) == 0 {
		return fmt.Errorf("%s declares no artifacts", label)
	}

	roles := make(map[string]ModelArtifact, len(manifest.Artifacts))
	for i, artifact := range manifest.Artifacts {
		artifactLabel := fmt.Sprintf("%s artifact %d", label, i)
		if artifact.Role == "" {
			return fmt.Errorf("%s has no role", artifactLabel)
		}
		if _, exists := roles[artifact.Role]; exists {
			return fmt.Errorf("%s declares duplicate artifact role %q", label, artifact.Role)
		}
		roles[artifact.Role] = artifact
		if _, err := cleanArtifactPath(artifact.Path); err != nil {
			return fmt.Errorf("%s role %q: %w", label, artifact.Role, err)
		}
		if artifact.MinSize < 0 {
			return fmt.Errorf("%s role %q has negative min_size", label, artifact.Role)
		}
		if artifact.SHA256 != "" {
			decoded, err := hex.DecodeString(artifact.SHA256)
			if err != nil || len(decoded) != sha256.Size {
				return fmt.Errorf("%s role %q has invalid sha256", label, artifact.Role)
			}
		}
		if !builtin && artifact.required() && artifact.SHA256 == "" {
			return fmt.Errorf("%s role %q requires sha256", label, artifact.Role)
		}
		for _, source := range artifact.Sources {
			if err := validateModelSourceURL(source.URL); err != nil {
				return fmt.Errorf("%s role %q: %w", label, artifact.Role, err)
			}
		}
	}
	for entrypoint, role := range manifest.Runtime.Entrypoints {
		if strings.TrimSpace(entrypoint) == "" || strings.TrimSpace(role) == "" {
			return fmt.Errorf("%s has an empty runtime entrypoint or role", label)
		}
		artifact, ok := roles[role]
		if !ok {
			return fmt.Errorf("%s entrypoint %q references unknown artifact role %q", label, entrypoint, role)
		}
		if !artifact.required() {
			return fmt.Errorf("%s entrypoint %q references optional artifact role %q", label, entrypoint, role)
		}
	}
	if _, ok := manifest.Runtime.Entrypoints["model"]; !ok {
		return fmt.Errorf("%s has no runtime entrypoint named model", label)
	}
	tokenizerArtifact, ok := roles[manifest.Tokenizer.Artifact]
	if !ok {
		return fmt.Errorf("%s tokenizer references unknown artifact role %q", label, manifest.Tokenizer.Artifact)
	}
	if !tokenizerArtifact.required() {
		return fmt.Errorf("%s tokenizer references optional artifact role %q", label, manifest.Tokenizer.Artifact)
	}
	if manifest.Tokenizer.Format == "" {
		return fmt.Errorf("%s tokenizer format is required", label)
	}
	if manifest.Tokenizer.Format != "huggingface-json" {
		return fmt.Errorf("%s tokenizer format %q is not supported by the current runtime", label, manifest.Tokenizer.Format)
	}
	if manifest.Text.MaxTokens <= 0 {
		return fmt.Errorf("%s text.max_tokens must be positive", label)
	}
	if manifest.Text.Truncation != "longest-first" {
		return fmt.Errorf("%s text.truncation %q is unsupported", label, manifest.Text.Truncation)
	}
	if manifest.Text.Padding != "longest" {
		return fmt.Errorf("%s text.padding %q is unsupported", label, manifest.Text.Padding)
	}
	inputNames := make(map[string]string, len(manifest.Inference.Inputs))
	for role, name := range manifest.Inference.Inputs {
		if !slices.Contains([]string{"input_ids", "attention_mask", "token_type_ids"}, role) {
			return fmt.Errorf("%s declares unsupported logical input role %q", label, role)
		}
		if name == "" {
			return fmt.Errorf("%s logical input role %q has an empty ONNX input name", label, role)
		}
		if previous, duplicate := inputNames[name]; duplicate {
			return fmt.Errorf("%s maps logical input roles %q and %q to ONNX input %q", label, previous, role, name)
		}
		inputNames[name] = role
	}

	switch task {
	case ModelTaskEmbedding:
		if manifest.Inference.Dimensions <= 0 {
			return fmt.Errorf("%s inference.dimensions must be positive for embeddings", label)
		}
		if !slices.Contains([]string{"none", "mean", "cls"}, manifest.Inference.Pooling) {
			return fmt.Errorf("%s inference.pooling %q is unsupported", label, manifest.Inference.Pooling)
		}
	case ModelTaskRerank:
		if manifest.Inference.MaxBatch <= 0 {
			return fmt.Errorf("%s inference.max_batch must be positive", label)
		}
		if !slices.Contains([]string{"auto", "none", "sigmoid", "softmax"}, manifest.Inference.ScoreTransform) {
			return fmt.Errorf("%s inference.score_transform %q is unsupported", label, manifest.Inference.ScoreTransform)
		}
		if manifest.Inference.PositiveClass != nil && *manifest.Inference.PositiveClass < 0 {
			return fmt.Errorf("%s inference.positive_class must not be negative", label)
		}
	default:
		return fmt.Errorf("%s task %q has no runtime adapter", label, task)
	}
	return nil
}

func validateModelSourceURL(raw string) error {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil || u.Host == "" || (u.Scheme != "https" && u.Scheme != "http") {
		return fmt.Errorf("artifact URL %q must be an absolute HTTP(S) URL", raw)
	}
	if u.User != nil {
		return fmt.Errorf("artifact URL %q must not contain credentials", raw)
	}
	if u.Scheme == "https" {
		return nil
	}
	host := strings.ToLower(u.Hostname())
	ip := net.ParseIP(host)
	if host != "localhost" && (ip == nil || !ip.IsLoopback()) {
		return fmt.Errorf("artifact URL %q must use HTTPS or an HTTP loopback address", raw)
	}
	return nil
}

func (artifact ModelArtifact) required() bool {
	return artifact.Required == nil || *artifact.Required
}

func (manifest ModelManifest) artifact(role string) (ModelArtifact, bool) {
	for _, artifact := range manifest.Artifacts {
		if artifact.Role == role {
			return artifact, true
		}
	}
	return ModelArtifact{}, false
}

func (manifest ModelManifest) compatibilityIdentity() string {
	data, _ := json.Marshal(manifest)
	digest := sha256.Sum256(data)
	return hex.EncodeToString(digest[:])
}
