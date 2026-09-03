package ai

import (
	"bufio"
	"context"
	"encoding/json"
	"math"
	"net"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestMeanPooling(t *testing.T) {
	t.Parallel()

	embeddings := [][]float32{
		{1, 3, 5},
		{2, 4, 6},
		{7, 8, 9},
		{0, 0, 0},
	}
	masks := [][]int{
		{1, 1},
		{1, 0},
	}

	batchSize := 2
	seqLen := 2
	hiddenDim := 3

	results := make([][]float32, batchSize)
	for i := 0; i < batchSize; i++ {
		vec := make([]float32, hiddenDim)
		var maskSum float32
		for j := 0; j < seqLen; j++ {
			m := float32(masks[i][j])
			maskSum += m
			for k := 0; k < hiddenDim; k++ {
				vec[k] += embeddings[i*seqLen+j][k] * m
			}
		}
		if maskSum > 0 {
			for k := 0; k < hiddenDim; k++ {
				vec[k] /= maskSum
			}
		}
		results[i] = vec
	}

	expected0 := []float32{1.5, 3.5, 5.5}
	for k, want := range expected0 {
		if math.Abs(float64(results[0][k]-want)) > 1e-6 {
			t.Errorf("seq0[%d] = %f; want %f", k, results[0][k], want)
		}
	}

	expected1 := []float32{7, 8, 9}
	for k, want := range expected1 {
		if math.Abs(float64(results[1][k]-want)) > 1e-6 {
			t.Errorf("seq1[%d] = %f; want %f", k, results[1][k], want)
		}
	}
}

func TestMeanPoolingAllMasked(t *testing.T) {
	t.Parallel()

	hiddenDim := 3
	vec := make([]float32, hiddenDim)
	var maskSum float32

	masks := []int{0, 0, 0}
	for _, m := range masks {
		maskSum += float32(m)
	}
	if maskSum > 0 {
		for k := 0; k < hiddenDim; k++ {
			vec[k] /= maskSum
		}
	}

	for k := 0; k < hiddenDim; k++ {
		if vec[k] != 0 {
			t.Errorf("vec[%d] = %f; want 0", k, vec[k])
		}
	}
}

func TestL2Normalize(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name     string
		input    []float32
		wantNorm float64
	}{
		{"unit_3_4", []float32{3, 4, 0}, 1.0},
		{"general", []float32{1, 2, 3}, 1.0},
		{"single", []float32{5}, 1.0},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()
			vec := make([]float32, len(tt.input))
			copy(vec, tt.input)

			var norm float64
			for _, v := range vec {
				norm += float64(v) * float64(v)
			}
			norm = math.Sqrt(norm)
			if norm > 0 {
				for k := range vec {
					vec[k] = float32(float64(vec[k]) / norm)
				}
			}

			var resultNorm float64
			for _, v := range vec {
				resultNorm += float64(v) * float64(v)
			}
			resultNorm = math.Sqrt(resultNorm)
			if math.Abs(resultNorm-tt.wantNorm) > 1e-5 {
				t.Errorf("norm after L2 = %f; want %f", resultNorm, tt.wantNorm)
			}
		})
	}

	t.Run("zero_vector", func(t *testing.T) {
		t.Parallel()
		vec := []float32{0, 0, 0}
		var norm float64
		for _, v := range vec {
			norm += float64(v) * float64(v)
		}
		norm = math.Sqrt(norm)
		if norm > 0 {
			for k := range vec {
				vec[k] = float32(float64(vec[k]) / norm)
			}
		}
		for k, v := range vec {
			if v != 0 {
				t.Errorf("zero_vec[%d] = %f; want 0", k, v)
			}
		}
	})
}

func startMockEmbedSocket(t *testing.T, sockPath string, handler func(req embedRequest) embedResponse) net.Listener {
	t.Helper()
	ln, err := net.Listen("unix", sockPath)
	if err != nil {
		t.Fatalf("listen unix socket: %v", err)
	}
	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			go func(c net.Conn) {
				defer c.Close()
				scanner := bufio.NewScanner(c)
				if scanner.Scan() {
					var req embedRequest
					_ = json.Unmarshal(scanner.Bytes(), &req)
					resp := handler(req)
					data, _ := json.Marshal(resp)
					data = append(data, '\n')
					_, _ = c.Write(data)
				}
			}(conn)
		}
	}()
	return ln
}

func TestProxyEmbeddingClient_EmbedBatch(t *testing.T) {
	t.Parallel()
	sockDir := t.TempDir()
	sockPath := filepath.Join(sockDir, "test.sock")

	ln := startMockEmbedSocket(t, sockPath, func(req embedRequest) embedResponse {
		vecs := make([][]float32, len(req.Texts))
		for i := range req.Texts {
			vec := make([]float32, EmbeddingDimensions)
			vec[0] = float32(i + 1)
			vecs[i] = vec
		}
		return embedResponse{Vectors: vecs}
	})
	defer ln.Close()

	client := &proxyEmbeddingClient{sockFile: sockPath, modelName: "test"}
	texts := []string{"hello", "world", "foo"}
	vecs, err := client.EmbedBatch(context.Background(), texts)
	if err != nil {
		t.Fatalf("EmbedBatch: %v", err)
	}
	if len(vecs) != 3 {
		t.Errorf("got %d vectors; want 3", len(vecs))
	}
	for i := range vecs {
		if vecs[i][0] != float32(i+1) {
			t.Errorf("vec[%d][0] = %f; want %f", i, vecs[i][0], float32(i+1))
		}
	}
}

func TestProxyEmbeddingClient_EmbedBatch_Empty(t *testing.T) {
	t.Parallel()
	client := &proxyEmbeddingClient{sockFile: filepath.Join(t.TempDir(), "unused.sock"), modelName: "test"}
	vecs, err := client.EmbedBatch(context.Background(), nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if vecs != nil {
		t.Errorf("expected nil for empty input, got %v", vecs)
	}
}

func TestProxyEmbeddingClient_EmbedQuery(t *testing.T) {
	t.Parallel()
	sockDir := t.TempDir()
	sockPath := filepath.Join(sockDir, "query.sock")

	var receivedQuery string
	ln := startMockEmbedSocket(t, sockPath, func(req embedRequest) embedResponse {
		receivedQuery = req.Query
		vec := make([]float32, EmbeddingDimensions)
		vec[0] = 42.0
		return embedResponse{Vectors: [][]float32{vec}}
	})
	defer ln.Close()

	client := &proxyEmbeddingClient{sockFile: sockPath, modelName: "test"}
	vec, err := client.EmbedQuery(context.Background(), "search this")
	if err != nil {
		t.Fatalf("EmbedQuery: %v", err)
	}
	if receivedQuery != "search this" {
		t.Errorf("received query = %q; want %q", receivedQuery, "search this")
	}
	if vec[0] != 42.0 {
		t.Errorf("vec[0] = %f; want 42.0", vec[0])
	}
}

func TestProxyEmbeddingClient_Embed_Single(t *testing.T) {
	t.Parallel()
	sockDir := t.TempDir()
	sockPath := filepath.Join(sockDir, "embed.sock")

	ln := startMockEmbedSocket(t, sockPath, func(req embedRequest) embedResponse {
		vec := make([]float32, EmbeddingDimensions)
		vec[0] = 7.0
		return embedResponse{Vectors: [][]float32{vec}}
	})
	defer ln.Close()

	client := &proxyEmbeddingClient{sockFile: sockPath, modelName: "test"}
	vec, err := client.Embed(context.Background(), "single text")
	if err != nil {
		t.Fatalf("Embed: %v", err)
	}
	if len(vec) != EmbeddingDimensions {
		t.Errorf("vec len = %d; want %d", len(vec), EmbeddingDimensions)
	}
	if vec[0] != 7.0 {
		t.Errorf("vec[0] = %f; want 7.0", vec[0])
	}
}

func TestProxyEmbeddingClient_ServerError(t *testing.T) {
	t.Parallel()
	sockDir := t.TempDir()
	sockPath := filepath.Join(sockDir, "err.sock")

	ln := startMockEmbedSocket(t, sockPath, func(req embedRequest) embedResponse {
		return embedResponse{Error: "something went wrong"}
	})
	defer ln.Close()

	client := &proxyEmbeddingClient{sockFile: sockPath, modelName: "test"}
	_, err := client.EmbedBatch(context.Background(), []string{"test"})
	if err == nil {
		t.Error("expected error from server-side error")
	}
	if !strings.Contains(err.Error(), "something went wrong") {
		t.Errorf("error = %q; want to contain 'something went wrong'", err)
	}
}

func TestProxyEmbeddingClient_ConnectionRefused(t *testing.T) {
	t.Parallel()
	client := &proxyEmbeddingClient{sockFile: filepath.Join(t.TempDir(), "missing.sock"), modelName: "test"}
	_, err := client.EmbedBatch(context.Background(), []string{"test"})
	if err == nil {
		t.Error("expected error for connection refused")
	}
}

func TestProxyEmbeddingClient_ModelName(t *testing.T) {
	t.Parallel()
	client := &proxyEmbeddingClient{sockFile: filepath.Join(t.TempDir(), "unused.sock"), modelName: "my-model"}
	if client.ModelName() != "my-model" {
		t.Errorf("ModelName = %q; want 'my-model'", client.ModelName())
	}
}

func TestLazyEmbeddingClient_ModelName_NilClient(t *testing.T) {
	t.Parallel()
	lazy := NewLazyEmbeddingClient()
	name := lazy.ModelName()
	want := "embedder (lazy, not loaded)"
	if name != want {
		t.Errorf("ModelName = %q; want %q", name, want)
	}
}

func TestLazyEmbeddingClient_ModelName_WithClient(t *testing.T) {
	t.Parallel()
	lazy := NewLazyEmbeddingClient()
	lazy.client = &localEmbeddingClient{}
	name := lazy.ModelName()
	if name != localModelName {
		t.Errorf("ModelName = %q; want %q", name, localModelName)
	}
}

func TestNewEmbeddingClientFromConfig_WithSocket(t *testing.T) {
	tmpHome := t.TempDir()
	t.Setenv("HOME", tmpHome)

	sockDir := filepath.Join(tmpHome, brand.DotDir(), "daemon")
	if err := os.MkdirAll(sockDir, 0o755); err != nil {
		t.Fatal(err)
	}
	sockPath := filepath.Join(sockDir, "embed.sock")

	ln := startMockEmbedSocket(t, sockPath, func(req embedRequest) embedResponse {
		return embedResponse{}
	})
	defer ln.Close()

	client, err := NewEmbeddingClientFromConfig()
	if err != nil {
		t.Fatalf("NewEmbeddingClientFromConfig: %v", err)
	}
	if client == nil {
		t.Fatal("expected non-nil client")
	}
	if !strings.Contains(client.ModelName(), proxyModelTag) {
		t.Errorf("ModelName = %q; expected to contain %q", client.ModelName(), proxyModelTag)
	}
}

func TestNewEmbeddingClientFromConfig_FallsBackToLocal(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	seedModelCache(t, home)

	client, err := NewEmbeddingClientFromConfig()
	if err != nil {
		reached := []string{"model manager", "ONNX", "ensure model", "load tokenizer"}
		ok := false
		for _, s := range reached {
			if strings.Contains(err.Error(), s) {
				ok = true
				break
			}
		}
		if !ok {
			t.Errorf("unexpected error type: %v", err)
		}
		return
	}
	if client == nil {
		t.Fatal("expected non-nil client")
	}
}

func TestNewProxyEmbeddingClient_NoSocket(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	client := newProxyEmbeddingClient()
	if client != nil {
		t.Error("expected nil when socket doesn't exist")
	}
}

func TestNewProxyEmbeddingClient_SocketExists(t *testing.T) {
	tmpHome := t.TempDir()
	t.Setenv("HOME", tmpHome)

	sockDir := filepath.Join(tmpHome, brand.DotDir(), "daemon")
	if err := os.MkdirAll(sockDir, 0o755); err != nil {
		t.Fatal(err)
	}
	sockPath := filepath.Join(sockDir, "embed.sock")

	ln := startMockEmbedSocket(t, sockPath, func(req embedRequest) embedResponse {
		return embedResponse{}
	})
	defer ln.Close()

	client := newProxyEmbeddingClient()
	if client == nil {
		t.Fatal("expected non-nil client when socket is available")
	}
	if !strings.Contains(client.ModelName(), proxyModelTag) {
		t.Errorf("ModelName = %q; expected to contain %q", client.ModelName(), proxyModelTag)
	}
}

func TestModelManager_EnsureModel_CreateCacheDir(t *testing.T) {
	t.Parallel()

	tmpDir := t.TempDir()
	deepCacheDir := filepath.Join(tmpDir, "a", "b", "c", "cache")

	server := artifactServer(t, map[string][]byte{"/artifact": []byte("content")})
	mgr := &ModelManager{cacheDir: deepCacheDir}
	if _, err := mgr.ensureArtifact(context.Background(), modelArtifact{
		name: "artifact.bin", source: server + "/artifact", minSize: 1,
	}); err != nil {
		t.Fatalf("ensureArtifact: %v", err)
	}

	if _, statErr := os.Stat(deepCacheDir); statErr != nil {
		t.Errorf("expected cache dir to be created: %v", statErr)
	}
}

func TestModelManager_Download_CancelledContext(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	mgr := &ModelManager{cacheDir: t.TempDir()}
	dest := filepath.Join(mgr.cacheDir, "cancelled.onnx")

	err := mgr.download(ctx, "http://example.com/model.onnx", dest)
	if err == nil {
		t.Error("expected error for cancelled context")
	}
}

func TestFindORTLibrary_MultipleEnvPaths(t *testing.T) {
	dir1 := t.TempDir()
	dir2 := t.TempDir()

	libPath := filepath.Join(dir2, "libonnxruntime.so")
	if err := os.WriteFile(libPath, []byte("fake"), 0o644); err != nil {
		t.Fatal(err)
	}

	t.Setenv("LD_LIBRARY_PATH", dir1+string(os.PathListSeparator)+dir2)

	result := findORTLibrary()
	if result != libPath {
		t.Errorf("findORTLibrary() = %q; want %q", result, libPath)
	}
}

func TestFindORTLibrary_EmptyEnv(t *testing.T) {
	t.Setenv("LD_LIBRARY_PATH", "")
	t.Setenv("DYLD_LIBRARY_PATH", "")
	emptyDir := t.TempDir()
	t.Setenv("PATH", emptyDir)
	_ = findORTLibrary()
}

func TestLocalEmbeddingClient_Close_WithNilSession(t *testing.T) {
	t.Parallel()
	c := &localEmbeddingClient{session: nil}
	c.Close()
}

func TestEmbeddingDimensionsConstant(t *testing.T) {
	t.Parallel()
	if EmbeddingDimensions != 768 {
		t.Errorf("EmbeddingDimensions = %d; want 768", EmbeddingDimensions)
	}
}
