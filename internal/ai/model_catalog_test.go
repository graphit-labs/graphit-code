package ai

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	ort "github.com/yalue/onnxruntime_go"
)

func testDigest(data []byte) string {
	digest := sha256.Sum256(data)
	return hex.EncodeToString(digest[:])
}

func testManifest(id string, task ModelTask, policy FetchPolicy, sources map[string]string) ModelManifest {
	model := []byte("model-content")
	tokenizer := []byte("tokenizer-content")
	manifest := ModelManifest{
		SchemaVersion: ModelManifestVersion,
		ID:            id,
		Name:          "Operator model",
		Revision:      "test-1",
		Task:          task,
		Runtime: ModelRuntimeConfig{
			Engine:      "onnx",
			Entrypoints: map[string]string{"model": "weights"},
		},
		FetchPolicy: policy,
		Artifacts: []ModelArtifact{
			{Role: "weights", Path: "onnx/weights.bin", SHA256: testDigest(model), MinSize: int64(len(model))},
			{Role: "vocabulary", Path: "tokenizers/main.json", Format: "huggingface-json", SHA256: testDigest(tokenizer), MinSize: int64(len(tokenizer))},
		},
		Tokenizer: ModelTokenizerConfig{Artifact: "vocabulary", Format: "huggingface-json"},
		Text: ModelTextConfig{
			MaxTokens:  256,
			Truncation: "longest-first",
			Padding:    "longest",
		},
		Inference: ModelInferenceConfig{
			Dimensions:     3,
			Pooling:        "none",
			MaxBatch:       4,
			ScoreTransform: "auto",
		},
	}
	for i := range manifest.Artifacts {
		if source := sources[manifest.Artifacts[i].Role]; source != "" {
			manifest.Artifacts[i].Sources = []ModelArtifactSource{{URL: source}}
		}
	}
	return manifest
}

func writeTestManifest(t *testing.T, globalDir string, manifest ModelManifest) string {
	t.Helper()
	dir := filepath.Join(globalDir, "models", manifest.ID)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	data, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, modelManifestFile), data, 0o644); err != nil {
		t.Fatal(err)
	}
	return dir
}

func TestBuiltInModelsUseTheManifestCatalog(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)

	embedding, err := LoadModel(ModelTaskEmbedding, DefaultEmbeddingModelID)
	if err != nil {
		t.Fatal(err)
	}
	if embedding.Dir != filepath.Join(globalDir, "models", DefaultEmbeddingModelID) {
		t.Fatalf("embedding dir = %q", embedding.Dir)
	}
	if embedding.Manifest.Runtime.Entrypoints["model"] != "model" || embedding.Manifest.Tokenizer.Artifact != "tokenizer" {
		t.Fatalf("embedding manifest does not describe its runtime artifacts: %#v", embedding.Manifest)
	}
	if embedding.Manifest.FetchPolicy != FetchPolicyOnDemand || embedding.Manifest.Inference.Dimensions != EmbeddingDimensions {
		t.Fatalf("unexpected embedding manifest policy/width: %#v", embedding.Manifest)
	}

	rerank, err := LoadModel(ModelTaskRerank, DefaultRerankModelID)
	if err != nil {
		t.Fatal(err)
	}
	if rerank.Manifest.FetchPolicy != FetchPolicyOnDemand || rerank.Manifest.Runtime.Engine != "onnx" {
		t.Fatalf("unexpected rerank manifest: %#v", rerank.Manifest)
	}
}

func TestModelManifestSchemaIsHardwareNeutral(t *testing.T) {
	encoded, err := json.Marshal(builtinModelManifests[DefaultEmbeddingModelID])
	if err != nil {
		t.Fatal(err)
	}
	text := strings.ToLower(string(encoded))
	for _, forbidden := range []string{"\"device\"", "device_id", "cuda", "coreml", "provider_library"} {
		if strings.Contains(text, forbidden) {
			t.Fatalf("manifest contains hardware field/value %q: %s", forbidden, text)
		}
	}
}

func TestUserManifestIsSelectedFromGlobalConfig(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	manifest := testManifest("my-embedder", ModelTaskEmbedding, FetchPolicyNever, nil)
	manifest.Text.QueryPrefix = "query: "
	manifest.Text.DocumentPrefix = "document: "
	manifest.Inference.Inputs = map[string]string{"input_ids": "ids", "attention_mask": "mask"}
	manifest.Inference.Output = "embeddings"
	manifest.Inference.Normalize = boolPointer(true)
	writeTestManifest(t, globalDir, manifest)
	if err := config.SetGlobalConfigValue(EmbeddingModelConfigKey, manifest.ID); err != nil {
		t.Fatal(err)
	}

	resolved, err := LoadConfiguredModel(ModelTaskEmbedding)
	if err != nil {
		t.Fatal(err)
	}
	if resolved.Manifest.ID != manifest.ID || resolved.Manifest.Name != manifest.Name {
		t.Fatalf("selected manifest = %#v", resolved.Manifest)
	}
	if resolved.Manifest.Text.QueryPrefix != "query: " || resolved.Manifest.Text.DocumentPrefix != "document: " ||
		resolved.Manifest.Inference.Inputs["input_ids"] != "ids" || resolved.Manifest.Inference.Output != "embeddings" ||
		resolved.Manifest.Inference.Normalize == nil || !*resolved.Manifest.Inference.Normalize || resolved.Manifest.Inference.Dimensions != 3 {
		t.Fatalf("embedding runtime semantics were not preserved: %#v", resolved.Manifest)
	}
	if resolved.Dir != filepath.Join(globalDir, "models", manifest.ID) {
		t.Fatalf("bundle dir = %q", resolved.Dir)
	}
}

func TestConfiguredModelDoesNotSilentlyFallback(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	if err := config.SetGlobalConfigValue(EmbeddingModelConfigKey, "missing-custom-model"); err != nil {
		t.Fatal(err)
	}
	_, err := LoadConfiguredModel(ModelTaskEmbedding)
	if err == nil || !strings.Contains(err.Error(), "missing-custom-model") || !strings.Contains(err.Error(), modelManifestFile) {
		t.Fatalf("missing custom model error = %v", err)
	}
}

func TestArtifactPathRejectsSymlinkEscape(t *testing.T) {
	root := t.TempDir()
	outside := t.TempDir()
	if err := os.Symlink(outside, filepath.Join(root, "linked")); err != nil {
		t.Fatal(err)
	}
	_, err := safeBundlePath(root, "linked/model.onnx")
	if err == nil || !strings.Contains(err.Error(), "symlink") {
		t.Fatalf("symlink path error = %v", err)
	}
}

func TestCustomManifestValidationIsStrictAndActionable(t *testing.T) {
	tests := []struct {
		name string
		edit func(*ModelManifest)
		want string
	}{
		{"path escape", func(m *ModelManifest) { m.Artifacts[0].Path = "../weights.bin" }, "escapes the model bundle"},
		{"missing integrity", func(m *ModelManifest) { m.Artifacts[0].SHA256 = "" }, "requires sha256"},
		{"unknown tokenizer", func(m *ModelManifest) { m.Tokenizer.Artifact = "missing" }, "unknown artifact role"},
		{"unsupported pooling", func(m *ModelManifest) { m.Inference.Pooling = "max" }, "pooling"},
		{"wrong task", func(m *ModelManifest) { m.Task = ModelTaskRerank }, "not \"embedding\""},
		{"insecure URL", func(m *ModelManifest) {
			m.Artifacts[0].Sources = []ModelArtifactSource{{URL: "http://models.example/weights"}}
		}, "HTTPS or an HTTP loopback"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			globalDir := t.TempDir()
			t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
			manifest := testManifest("invalid-model", ModelTaskEmbedding, FetchPolicyNever, nil)
			tc.edit(&manifest)
			writeTestManifest(t, globalDir, manifest)
			_, err := LoadModel(ModelTaskEmbedding, manifest.ID)
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("error = %v, want it to contain %q", err, tc.want)
			}
		})
	}
}

func TestCustomManifestRejectsUnknownJSONFields(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	dir := filepath.Join(globalDir, "models", "unknown-field")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	data := `{"schema_version":1,"id":"unknown-field","task":"embedding","surprise":true}`
	if err := os.WriteFile(filepath.Join(dir, modelManifestFile), []byte(data), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err := LoadModel(ModelTaskEmbedding, "unknown-field")
	if err == nil || !strings.Contains(err.Error(), "unknown field") {
		t.Fatalf("error = %v", err)
	}
}

func TestArtifactResolverDownloadsValidatesAndCachesArbitraryPaths(t *testing.T) {
	modelData := []byte("model-content")
	tokenizerData := []byte("tokenizer-content")
	server := artifactServer(t, map[string][]byte{"/weights": modelData, "/tokenizer": tokenizerData})
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	manifest := testManifest("downloadable", ModelTaskEmbedding, FetchPolicyOnDemand, map[string]string{
		"weights": server + "/weights", "vocabulary": server + "/tokenizer",
	})
	optional := false
	manifest.Artifacts = append(manifest.Artifacts, ModelArtifact{Role: "labels", Path: "metadata/labels.json", Required: &optional})
	writeTestManifest(t, globalDir, manifest)

	resolved, err := LoadModel(ModelTaskEmbedding, manifest.ID)
	if err != nil {
		t.Fatal(err)
	}
	var progress []string
	resolved.OnProgress = func(file string, _, _ int64) { progress = append(progress, file) }
	paths, err := resolved.Ensure(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if string(mustReadFile(t, paths["weights"])) != string(modelData) || string(mustReadFile(t, paths["vocabulary"])) != string(tokenizerData) {
		t.Fatalf("downloaded artifacts do not match their sources")
	}
	if _, exists := paths["labels"]; exists {
		t.Fatal("absent optional artifact was returned as resolved")
	}
	if !resolved.Present() {
		t.Fatal("resolved bundle is not present after successful downloads")
	}
	if len(progress) < 4 || progress[0] != "onnx/weights.bin" {
		t.Fatalf("progress = %#v", progress)
	}

	resolved.OnProgress = func(string, int64, int64) { t.Fatal("cache hit reported download progress") }
	if _, err := resolved.Ensure(context.Background()); err != nil {
		t.Fatal(err)
	}
}

func TestArtifactResolverHonorsNeverAndLeavesNoInvalidDownload(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	server := artifactServer(t, map[string][]byte{"/weights": []byte("wrong-content"), "/tokenizer": []byte("tokenizer-content")})

	never := testManifest("never-fetch", ModelTaskEmbedding, FetchPolicyNever, map[string]string{
		"weights": server + "/weights", "vocabulary": server + "/tokenizer",
	})
	writeTestManifest(t, globalDir, never)
	resolved, err := LoadModel(ModelTaskEmbedding, never.ID)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := resolved.Ensure(context.Background()); err == nil || !strings.Contains(err.Error(), "fetch_policy is never") {
		t.Fatalf("never error = %v", err)
	}

	bad := testManifest("bad-download", ModelTaskEmbedding, FetchPolicyOnDemand, map[string]string{
		"weights": server + "/weights", "vocabulary": server + "/tokenizer",
	})
	writeTestManifest(t, globalDir, bad)
	badResolved, err := LoadModel(ModelTaskEmbedding, bad.ID)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := badResolved.Ensure(context.Background()); err == nil || !strings.Contains(err.Error(), "sha256") {
		t.Fatalf("integrity error = %v", err)
	}
	badPath, _ := badResolved.ArtifactPath("weights")
	if _, err := os.Stat(badPath); !os.IsNotExist(err) {
		t.Fatalf("invalid artifact was published at %s", badPath)
	}
	temps, err := filepath.Glob(filepath.Join(filepath.Dir(badPath), ".weights.bin-*.tmp"))
	if err != nil || len(temps) != 0 {
		t.Fatalf("temporary files left behind: %v, %v", temps, err)
	}
}

func TestOptionalArtifactFailureDoesNotBlockRequiredBundle(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	server := artifactServer(t, map[string][]byte{"/weights": []byte("model-content"), "/tokenizer": []byte("tokenizer-content")})
	manifest := testManifest("optional-companion", ModelTaskEmbedding, FetchPolicyOnDemand, map[string]string{
		"weights": server + "/weights", "vocabulary": server + "/tokenizer",
	})
	optional := false
	manifest.Artifacts = append(manifest.Artifacts, ModelArtifact{
		Role: "labels", Path: "metadata/labels.json", Required: &optional,
		Sources: []ModelArtifactSource{{URL: server + "/missing-labels"}},
	})
	writeTestManifest(t, globalDir, manifest)
	resolved, err := LoadModel(ModelTaskEmbedding, manifest.ID)
	if err != nil {
		t.Fatal(err)
	}
	paths, err := resolved.Ensure(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if len(paths) != 2 || !resolved.Present() {
		t.Fatalf("required bundle did not resolve: paths=%#v present=%t", paths, resolved.Present())
	}
}

func TestCustomRerankManifestCanDownloadFromFallbackSource(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	server := artifactServer(t, map[string][]byte{"/weights": []byte("model-content"), "/tokenizer": []byte("tokenizer-content")})
	manifest := testManifest("operator-reranker", ModelTaskRerank, FetchPolicyOnDemand, map[string]string{
		"vocabulary": server + "/tokenizer",
	})
	manifest.Artifacts[0].Sources = []ModelArtifactSource{{URL: server + "/missing"}, {URL: server + "/weights"}}
	writeTestManifest(t, globalDir, manifest)
	if err := config.SetGlobalConfigValue(RerankModelConfigKey, manifest.ID); err != nil {
		t.Fatal(err)
	}
	resolved, err := LoadConfiguredModel(ModelTaskRerank)
	if err != nil {
		t.Fatal(err)
	}
	paths, err := resolved.Ensure(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if resolved.Manifest.ID != manifest.ID || string(mustReadFile(t, paths["weights"])) != "model-content" {
		t.Fatalf("custom rerank did not resolve: manifest=%q paths=%#v", resolved.Manifest.ID, paths)
	}
}

func TestSetupPolicyPrefetchesOnlySetupModels(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	server := artifactServer(t, map[string][]byte{"/weights": []byte("model-content"), "/tokenizer": []byte("tokenizer-content")})
	manifest := testManifest("setup-model", ModelTaskEmbedding, FetchPolicySetup, map[string]string{
		"weights": server + "/weights", "vocabulary": server + "/tokenizer",
	})
	writeTestManifest(t, globalDir, manifest)
	if err := config.SetGlobalConfigValue(EmbeddingModelConfigKey, manifest.ID); err != nil {
		t.Fatal(err)
	}
	if err := PrefetchConfiguredLocalModels(context.Background()); err != nil {
		t.Fatal(err)
	}
	resolved, err := LoadConfiguredModel(ModelTaskEmbedding)
	if err != nil || !resolved.Present() {
		t.Fatalf("setup model was not prefetched: present=%v error=%v", resolved != nil && resolved.Present(), err)
	}
}

func TestEmbeddingIdentityIncludesCompatibilityInputs(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	manifest := testManifest("identity-model", ModelTaskEmbedding, FetchPolicyNever, nil)
	manifest.Text.QueryPrefix = "query: "
	writeTestManifest(t, globalDir, manifest)
	if err := config.SetGlobalConfigValue(EmbeddingModelConfigKey, manifest.ID); err != nil {
		t.Fatal(err)
	}
	p1, id1, width1 := configuredLocalEmbeddingIdentity()
	manifest.Text.QueryPrefix = "search: "
	writeTestManifest(t, globalDir, manifest)
	p2, id2, width2 := configuredLocalEmbeddingIdentity()
	if p1 != "local" || p2 != "local" || width1 != 3 || width2 != 3 || id1 == id2 {
		t.Fatalf("identities: (%q,%q,%d), (%q,%q,%d)", p1, id1, width1, p2, id2, width2)
	}
}

func TestEmbeddingPoolingAndNormalization(t *testing.T) {
	tests := []struct {
		name      string
		pooling   string
		normalize bool
		shape     ort.Shape
		data      []float32
		mask      []int
		want      []float32
	}{
		{"direct", "none", false, ort.Shape{1, 2}, []float32{3, 4}, nil, []float32{3, 4}},
		{"normalized", "none", true, ort.Shape{1, 2}, []float32{3, 4}, nil, []float32{0.6, 0.8}},
		{"mean", "mean", false, ort.Shape{1, 3, 2}, []float32{1, 2, 3, 4, 100, 200}, []int{1, 1, 0}, []float32{2, 3}},
		{"cls", "cls", false, ort.Shape{1, 2, 2}, []float32{7, 8, 9, 10}, []int{1, 1}, []float32{7, 8}},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			client := localEmbeddingClient{dimensions: 2, pooling: tc.pooling, normalize: tc.normalize, output: "out"}
			got, err := client.poolEmbedding(tc.data, tc.shape, 0, 1, tc.mask)
			if err != nil {
				t.Fatal(err)
			}
			for i := range tc.want {
				if math.Abs(float64(got[i]-tc.want[i])) > 1e-6 {
					t.Fatalf("pool result = %v, want %v", got, tc.want)
				}
			}
		})
	}
}

func TestONNXInputAndOutputSelection(t *testing.T) {
	if got := resolveInputRole("ids", map[string]string{"input_ids": "ids"}); got != "input_ids" {
		t.Fatalf("override role = %q", got)
	}
	if got := resolveInputRole("attention_mask", nil); got != "attention_mask" {
		t.Fatalf("discovered role = %q", got)
	}
	manifest := ModelManifest{ID: "reranker", Task: ModelTaskRerank}
	outputs := []ort.InputOutputInfo{{Name: "hidden"}, {Name: "logits"}}
	output, err := selectONNXOutput(manifest, outputs)
	if err != nil || output.Name != "logits" {
		t.Fatalf("output = %#v, error = %v", output, err)
	}
	manifest.Inference.Output = "missing"
	if _, err := selectONNXOutput(manifest, outputs); err == nil || !strings.Contains(err.Error(), "missing ONNX output") {
		t.Fatalf("missing output error = %v", err)
	}
}

func mustReadFile(t *testing.T, path string) []byte {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return data
}
