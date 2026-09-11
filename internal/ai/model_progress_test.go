package ai

import (
	"context"
	"os"
	"path/filepath"
	"testing"
)

type progressCall struct {
	file       string
	downloaded int64
	total      int64
}

func TestResolvedModelReportsDownloadProgress(t *testing.T) {
	content := []byte("progress-content")
	server := artifactServer(t, map[string][]byte{"/nested/artifact": content})
	destination := filepath.Join(t.TempDir(), "nested", "artifact.bin")
	if err := os.MkdirAll(filepath.Dir(destination), 0o755); err != nil {
		t.Fatal(err)
	}
	artifact := ModelArtifact{
		Role: "weights", Path: "nested/artifact.bin", MinSize: int64(len(content)), SHA256: testDigest(content),
	}
	var calls []progressCall
	model := &ResolvedModel{OnProgress: func(file string, downloaded, total int64) {
		calls = append(calls, progressCall{file: file, downloaded: downloaded, total: total})
	}}
	if err := model.downloadArtifact(context.Background(), server+"/nested/artifact", destination, artifact); err != nil {
		t.Fatal(err)
	}
	if len(calls) < 2 {
		t.Fatalf("progress calls = %#v", calls)
	}
	if first := calls[0]; first.file != artifact.Path || first.downloaded != 0 || first.total != int64(len(content)) {
		t.Fatalf("first progress = %#v", first)
	}
	if last := calls[len(calls)-1]; last.downloaded != int64(len(content)) || last.total != int64(len(content)) {
		t.Fatalf("last progress = %#v", last)
	}
	for i := 1; i < len(calls); i++ {
		if calls[i].downloaded < calls[i-1].downloaded {
			t.Fatalf("progress moved backwards: %#v", calls)
		}
	}
}

func TestResolvedModelDownloadHonorsCancelledContext(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	artifact := ModelArtifact{Role: "weights", Path: "weights.bin", SHA256: testDigest([]byte("unused"))}
	model := &ResolvedModel{}
	destination := filepath.Join(t.TempDir(), artifact.Path)
	if err := model.downloadArtifact(ctx, "http://127.0.0.1:1/weights", destination, artifact); err == nil {
		t.Fatal("cancelled download returned no error")
	}
	if _, err := os.Stat(destination); !os.IsNotExist(err) {
		t.Fatalf("cancelled download published a destination: %v", err)
	}
}
