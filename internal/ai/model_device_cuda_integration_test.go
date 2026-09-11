package ai

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	ort "github.com/yalue/onnxruntime_go"
)

// This test is opt-in because CI is not required to have an NVIDIA GPU. It exercises Graphit's
// provider-aware session factory, executes an ONNX graph, and verifies from ONNX Runtime profiling
// that at least one node actually ran on CUDA rather than merely loading the provider library.
func TestCUDADeviceIntegration(t *testing.T) {
	if os.Getenv("GRAPHIT_TEST_CUDA") != "1" {
		t.Skip("set GRAPHIT_TEST_CUDA=1 and GRAPHIT_TEST_CUDA_MODEL to run on an NVIDIA host")
	}
	modelPath := os.Getenv("GRAPHIT_TEST_CUDA_MODEL")
	if modelPath == "" {
		t.Fatal("GRAPHIT_TEST_CUDA_MODEL is required")
	}
	if err := initONNXRuntime(); err != nil {
		t.Fatalf("initialize ONNX Runtime: %v", err)
	}

	profilePrefix := filepath.Join(t.TempDir(), "graphit-cuda")
	session, selected, err := newDynamicONNXSession(
		modelPath,
		[]string{"in"},
		[]string{"out"},
		ONNXExecutionConfig{Device: ONNXDeviceCUDA, DeviceID: 0},
		func(opts *ort.SessionOptions) error { return opts.EnableProfiling(profilePrefix) },
	)
	if err != nil {
		t.Fatalf("create CUDA session: %v", err)
	}
	if selected != ONNXDeviceCUDA {
		t.Fatalf("selected device = %q", selected)
	}

	input, err := ort.NewTensor(ort.NewShape(1, 2), []int32{12, 21})
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = input.Destroy() }()
	output, err := ort.NewEmptyTensor[int32](ort.NewShape(1))
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = output.Destroy() }()
	if err := session.Run([]ort.Value{input}, []ort.Value{output}); err != nil {
		t.Fatalf("run CUDA session: %v", err)
	}
	if got := output.GetData()[0]; got != 33 {
		t.Fatalf("CUDA inference output = %d, want 33", got)
	}
	if err := session.Destroy(); err != nil {
		t.Fatalf("destroy CUDA session: %v", err)
	}

	profiles, err := filepath.Glob(profilePrefix + "*.json")
	if err != nil || len(profiles) != 1 {
		t.Fatalf("CUDA profile files = %#v, error = %v", profiles, err)
	}
	profile, err := os.ReadFile(profiles[0])
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(profile), "CUDAExecutionProvider") {
		t.Fatalf("ONNX profile does not show CUDA execution: %s", profiles[0])
	}
	t.Logf("CUDA inference succeeded on device 0; profile: %s", profiles[0])
}
