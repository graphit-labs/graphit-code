package main

import (
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"testing"
	"testing/fstest"
)

func TestRequiredONNXRuntimeFiles(t *testing.T) {
	tests := []struct {
		goos string
		want []string
	}{
		{"linux", []string{"libonnxruntime.so.1.29.0", "libonnxruntime_providers_shared.so", "libonnxruntime_providers_cuda.so"}},
		{"windows", []string{"onnxruntime.dll", "onnxruntime_providers_shared.dll", "onnxruntime_providers_cuda.dll"}},
		{"darwin", []string{"libonnxruntime.1.29.0.dylib"}},
	}
	for _, tc := range tests {
		if got := requiredONNXRuntimeFiles(tc.goos); !reflect.DeepEqual(got, tc.want) {
			t.Fatalf("%s files = %#v, want %#v", tc.goos, got, tc.want)
		}
	}
}

func TestExtractRuntimeIncludesRequiredONNXLibraries(t *testing.T) {
	for _, goos := range []string{"linux", "windows", "darwin"} {
		t.Run(goos, func(t *testing.T) {
			source := fstest.MapFS{"runtime": {Mode: fs.ModeDir}}
			for _, name := range requiredONNXRuntimeFiles(goos) {
				source[filepath.ToSlash(filepath.Join("runtime", name))] = &fstest.MapFile{Data: []byte("native-library")}
			}
			runtimeDir := t.TempDir()
			if err := extractRuntimeFS(source, runtimeDir); err != nil {
				t.Fatal(err)
			}
			if err := validateONNXRuntimeFiles(runtimeDir, goos); err != nil {
				t.Fatal(err)
			}
			for _, name := range requiredONNXRuntimeFiles(goos) {
				data, err := os.ReadFile(filepath.Join(runtimeDir, name))
				if err != nil {
					t.Fatal(err)
				}
				if string(data) != "native-library" {
					t.Fatalf("%s extracted contents = %q", name, data)
				}
			}
		})
	}
}

func TestValidateONNXRuntimeFilesReportsMissingProvider(t *testing.T) {
	runtimeDir := t.TempDir()
	for _, name := range requiredONNXRuntimeFiles("linux")[:2] {
		if err := os.WriteFile(filepath.Join(runtimeDir, name), []byte("native-library"), fs.FileMode(0o755)); err != nil {
			t.Fatal(err)
		}
	}
	if err := validateONNXRuntimeFiles(runtimeDir, "linux"); err == nil {
		t.Fatal("expected missing CUDA provider to fail validation")
	}
}
