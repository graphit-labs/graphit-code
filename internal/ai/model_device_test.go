package ai

import (
	"errors"
	"reflect"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestConfiguredONNXExecutionIsIndependentPerService(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	embeddingExecution := auth.ONNXExecutionConfig{Device: auth.ONNXDeviceCUDA, DeviceID: 2}
	rerankExecution := auth.ONNXExecutionConfig{Device: auth.ONNXDeviceCPU, DeviceID: 0}
	provider := auth.Provider{
		Name: "gpu", Type: auth.ProviderLocal, Local: &auth.LocalConfig{},
		AI: auth.AIConfig{
			Embedding: auth.AIServiceConfig{Mode: auth.ServiceLocal, ONNX: &embeddingExecution},
			Rerank:    auth.AIServiceConfig{Mode: auth.ServiceLocal, ONNX: &rerankExecution},
		},
	}
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := store.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "local", Provider: "gpu", Username: "alice"}); err != nil {
		t.Fatal(err)
	}
	embedding, err := configuredONNXExecution(ModelTaskEmbedding)
	if err != nil {
		t.Fatal(err)
	}
	rerank, err := configuredONNXExecution(ModelTaskRerank)
	if err != nil {
		t.Fatal(err)
	}
	if embedding != (ONNXExecutionConfig{Device: ONNXDeviceCUDA, DeviceID: 2}) {
		t.Fatalf("embedding execution = %#v", embedding)
	}
	if rerank != (ONNXExecutionConfig{Device: ONNXDeviceCPU, DeviceID: 0}) {
		t.Fatalf("rerank execution = %#v", rerank)
	}
}

func TestConfiguredONNXExecutionDefaultsToAutoDeviceZero(t *testing.T) {
	globalDir := t.TempDir()
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if _, _, err := store.EnsureDefaultLocalProvider(); err != nil {
		t.Fatal(err)
	}
	if err := store.RemoveProvider(auth.DefaultLocalProviderName, false); err != nil {
		t.Fatal(err)
	}
	got, err := configuredONNXExecution(ModelTaskEmbedding)
	if err != nil {
		t.Fatal(err)
	}
	if got != (ONNXExecutionConfig{Device: ONNXDeviceAuto, DeviceID: 0}) {
		t.Fatalf("execution = %#v", got)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	provider, ok := state.Providers[auth.DefaultLocalProviderName]
	if !ok || provider.AI.Embedding.ONNX == nil || *provider.AI.Embedding.ONNX != got {
		t.Fatalf("resolved execution was not persisted: %#v", state)
	}
	if state.ActiveProfile != "" || len(state.Profiles) != 0 {
		t.Fatalf("resolution created a profile: %#v", state)
	}
}

func TestConfiguredONNXExecutionUsesActiveProviderWithoutCreatingLocal(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	cpu := auth.ONNXExecutionConfig{Device: auth.ONNXDeviceCPU, DeviceID: 3}
	store, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := store.AddProvider(auth.Provider{
		Name: "chosen", Type: auth.ProviderLocal, Local: &auth.LocalConfig{},
		AI: auth.AIConfig{Embedding: auth.AIServiceConfig{Mode: auth.ServiceLocal, ONNX: &cpu}},
	}); err != nil {
		t.Fatal(err)
	}
	if err := store.Login(auth.Profile{Name: "alice", Provider: "chosen", Username: "alice"}); err != nil {
		t.Fatal(err)
	}
	got, err := configuredONNXExecution(ModelTaskEmbedding)
	if err != nil {
		t.Fatal(err)
	}
	if got != cpu {
		t.Fatalf("execution = %#v", got)
	}
	state, err := store.Load()
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := state.Providers[auth.DefaultLocalProviderName]; ok {
		t.Fatal("local provider was created while another profile was active")
	}
}

func TestConfiguredONNXExecutionRejectsNonNumericDeviceID(t *testing.T) {
	_, err := ParseONNXExecution("cuda", "gpu-zero")
	if err == nil || !strings.Contains(err.Error(), "non-negative integer") {
		t.Fatalf("error = %v", err)
	}
}

func TestValidateONNXExecution(t *testing.T) {
	tests := []struct {
		name string
		cfg  ONNXExecutionConfig
		goos string
		want string
	}{
		{"unknown", ONNXExecutionConfig{Device: "metal"}, "darwin", "unknown device"},
		{"negative id", ONNXExecutionConfig{Device: ONNXDeviceCUDA, DeviceID: -1}, "linux", "non-negative"},
		{"coreml linux", ONNXExecutionConfig{Device: ONNXDeviceCoreML}, "linux", "only valid on macOS"},
		{"coreml nonzero id", ONNXExecutionConfig{Device: ONNXDeviceCoreML, DeviceID: 1}, "darwin", "requires device_id 0"},
		{"auto coreml nonzero id", ONNXExecutionConfig{Device: ONNXDeviceAuto, DeviceID: 1}, "darwin", "device_id must be 0"},
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			err := validateONNXExecution(tc.cfg, tc.goos)
			if err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("error = %v, want substring %q", err, tc.want)
			}
		})
	}
}

func TestAutoDeviceCandidatesArePlatformSpecific(t *testing.T) {
	tests := []struct {
		goos string
		want []ONNXDevice
	}{
		{"darwin", []ONNXDevice{ONNXDeviceCoreML, ONNXDeviceCPU}},
		{"linux", []ONNXDevice{ONNXDeviceCUDA, ONNXDeviceCPU}},
		{"windows", []ONNXDevice{ONNXDeviceCUDA, ONNXDeviceCPU}},
	}
	for _, tc := range tests {
		got, err := onnxDeviceCandidates(ONNXExecutionConfig{Device: ONNXDeviceAuto}, tc.goos)
		if err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(got, tc.want) {
			t.Fatalf("%s candidates = %#v, want %#v", tc.goos, got, tc.want)
		}
	}
}

func TestAutoFallsBackToCPU(t *testing.T) {
	var attempts []ONNXDevice
	selected, err := tryONNXDevices(ONNXExecutionConfig{Device: ONNXDeviceAuto, DeviceID: 3}, "linux", func(device ONNXDevice, deviceID int) error {
		attempts = append(attempts, device)
		if deviceID != 3 {
			t.Fatalf("device id = %d", deviceID)
		}
		if device == ONNXDeviceCUDA {
			return errors.New("CUDA unavailable")
		}
		return nil
	})
	if err != nil || selected != ONNXDeviceCPU {
		t.Fatalf("selected=%q err=%v", selected, err)
	}
	if !reflect.DeepEqual(attempts, []ONNXDevice{ONNXDeviceCUDA, ONNXDeviceCPU}) {
		t.Fatalf("attempts = %#v", attempts)
	}
}

func TestExplicitAcceleratorFailsClosed(t *testing.T) {
	for _, device := range []ONNXDevice{ONNXDeviceCUDA, ONNXDeviceCoreML} {
		goos := "linux"
		if device == ONNXDeviceCoreML {
			goos = "darwin"
		}
		var attempts []ONNXDevice
		_, err := tryONNXDevices(ONNXExecutionConfig{Device: device}, goos, func(candidate ONNXDevice, _ int) error {
			attempts = append(attempts, candidate)
			return errors.New("provider unavailable")
		})
		if err == nil || !strings.Contains(err.Error(), string(device)) {
			t.Fatalf("%s error = %v", device, err)
		}
		if !reflect.DeepEqual(attempts, []ONNXDevice{device}) {
			t.Fatalf("%s attempts = %#v", device, attempts)
		}
	}
}

func TestCPUDoesNotAttemptAnAccelerator(t *testing.T) {
	var attempts []ONNXDevice
	selected, err := tryONNXDevices(ONNXExecutionConfig{Device: ONNXDeviceCPU, DeviceID: 7}, "linux", func(device ONNXDevice, _ int) error {
		attempts = append(attempts, device)
		return nil
	})
	if err != nil || selected != ONNXDeviceCPU || !reflect.DeepEqual(attempts, []ONNXDevice{ONNXDeviceCPU}) {
		t.Fatalf("selected=%q attempts=%#v err=%v", selected, attempts, err)
	}
}
