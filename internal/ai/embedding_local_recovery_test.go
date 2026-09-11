package ai

import (
	"errors"
	"fmt"
	"sync"
	"testing"
	"time"

	ort "github.com/yalue/onnxruntime_go"
)

type scriptedEmbeddingSession struct {
	mu        sync.Mutex
	errors    []error
	runs      int
	destroyed int
}

func (s *scriptedEmbeddingSession) Run(_ []ort.Value, _ []ort.Value) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.runs++
	if len(s.errors) == 0 {
		return nil
	}
	err := s.errors[0]
	s.errors = s.errors[1:]
	return err
}

func (s *scriptedEmbeddingSession) Destroy() error {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.destroyed++
	return nil
}

func (s *scriptedEmbeddingSession) counts() (runs, destroyed int) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.runs, s.destroyed
}

func TestAutoEmbeddingFallsBackRecoversAcceleratorAndFallsBackAgain(t *testing.T) {
	now := time.Date(2026, 9, 8, 8, 0, 0, 0, time.UTC)
	oom := errors.New("BFCArena::AllocateRawInternal failed to allocate memory")
	gpu1 := &scriptedEmbeddingSession{errors: []error{oom}}
	cpu1 := &scriptedEmbeddingSession{}
	gpu2 := &scriptedEmbeddingSession{errors: []error{nil, oom}}
	cpu2 := &scriptedEmbeddingSession{}

	var cpuCreations, acceleratorCreations int
	client := &localEmbeddingClient{
		session:     gpu1,
		execution:   ONNXExecutionConfig{Device: ONNXDeviceAuto, DeviceID: 2},
		device:      ONNXDeviceCUDA,
		accelerator: ONNXDeviceCUDA,
		now:         func() time.Time { return now },
	}
	client.newSession = func(cfg ONNXExecutionConfig) (embeddingSession, ONNXDevice, error) {
		switch cfg.Device {
		case ONNXDeviceCPU:
			cpuCreations++
			if cpuCreations == 1 {
				return cpu1, ONNXDeviceCPU, nil
			}
			return cpu2, ONNXDeviceCPU, nil
		case ONNXDeviceCUDA:
			acceleratorCreations++
			if cfg.DeviceID != 2 {
				t.Fatalf("accelerator device ID = %d, want 2", cfg.DeviceID)
			}
			return gpu2, ONNXDeviceCUDA, nil
		default:
			return nil, "", fmt.Errorf("unexpected device %q", cfg.Device)
		}
	}

	if err := client.runONNX(nil, []ort.Value{nil}); err != nil {
		t.Fatalf("fallback call: %v", err)
	}
	if client.device != ONNXDeviceCPU || cpuCreations != 1 || acceleratorCreations != 0 {
		t.Fatalf("after fallback: device=%s cpu=%d accelerator=%d", client.device, cpuCreations, acceleratorCreations)
	}
	if _, destroyed := gpu1.counts(); destroyed != 1 {
		t.Fatalf("old accelerator destroyed %d times, want 1", destroyed)
	}

	// CPU remains available and no accelerator probe happens before the backoff.
	now = now.Add(embeddingAcceleratorRetryInitial - time.Second)
	if err := client.runONNX(nil, []ort.Value{nil}); err != nil {
		t.Fatalf("CPU call during backoff: %v", err)
	}
	if acceleratorCreations != 0 {
		t.Fatalf("accelerator was probed before backoff: %d creations", acceleratorCreations)
	}

	// The pending operation validates the new accelerator session before promotion.
	now = now.Add(time.Second)
	if err := client.runONNX(nil, []ort.Value{nil}); err != nil {
		t.Fatalf("accelerator recovery call: %v", err)
	}
	if client.device != ONNXDeviceCUDA || acceleratorCreations != 1 {
		t.Fatalf("after recovery: device=%s accelerator=%d", client.device, acceleratorCreations)
	}
	if _, destroyed := cpu1.counts(); destroyed != 1 {
		t.Fatalf("old CPU session destroyed %d times, want 1", destroyed)
	}

	// A later resource failure repeats that same operation on a fresh CPU session.
	if err := client.runONNX(nil, []ort.Value{nil}); err != nil {
		t.Fatalf("second fallback call: %v", err)
	}
	if client.device != ONNXDeviceCPU || cpuCreations != 2 {
		t.Fatalf("after second fallback: device=%s cpu=%d", client.device, cpuCreations)
	}
	if _, destroyed := gpu2.counts(); destroyed != 1 {
		t.Fatalf("recovered accelerator destroyed %d times, want 1", destroyed)
	}
}

func TestAutoEmbeddingProbeFailureUsesExponentialBackoff(t *testing.T) {
	now := time.Date(2026, 9, 8, 8, 0, 0, 0, time.UTC)
	cpu := &scriptedEmbeddingSession{}
	probes := 0
	client := &localEmbeddingClient{
		session:     cpu,
		execution:   ONNXExecutionConfig{Device: ONNXDeviceAuto},
		device:      ONNXDeviceCPU,
		accelerator: ONNXDeviceCUDA,
		now:         func() time.Time { return now },
		retryAt:     now.Add(embeddingAcceleratorRetryInitial),
		retryDelay:  embeddingAcceleratorRetryInitial,
		newSession: func(cfg ONNXExecutionConfig) (embeddingSession, ONNXDevice, error) {
			probes++
			return nil, "", errors.New("CUDA provider is still unavailable")
		},
	}

	now = now.Add(embeddingAcceleratorRetryInitial)
	if err := client.runONNX(nil, []ort.Value{nil}); err != nil {
		t.Fatalf("CPU call after failed probe: %v", err)
	}
	if probes != 1 || client.retryDelay != 2*embeddingAcceleratorRetryInitial {
		t.Fatalf("after failed probe: probes=%d delay=%s", probes, client.retryDelay)
	}

	now = now.Add(embeddingAcceleratorRetryInitial)
	if err := client.runONNX(nil, []ort.Value{nil}); err != nil {
		t.Fatalf("CPU call inside expanded backoff: %v", err)
	}
	if probes != 1 {
		t.Fatalf("accelerator reprobed too early: %d probes", probes)
	}

	now = now.Add(embeddingAcceleratorRetryInitial)
	if err := client.runONNX(nil, []ort.Value{nil}); err != nil {
		t.Fatalf("CPU call at expanded backoff: %v", err)
	}
	if probes != 2 {
		t.Fatalf("accelerator probes = %d, want 2", probes)
	}
}

func TestEmbeddingFallbackRequiresAutoAndResourceError(t *testing.T) {
	tests := []struct {
		name      string
		execution ONNXExecutionConfig
		device    ONNXDevice
		runErr    error
	}{
		{
			name:      "explicit CUDA fails closed",
			execution: ONNXExecutionConfig{Device: ONNXDeviceCUDA},
			device:    ONNXDeviceCUDA,
			runErr:    errors.New("CUDA out of memory"),
		},
		{
			name:      "auto does not hide model errors",
			execution: ONNXExecutionConfig{Device: ONNXDeviceAuto},
			device:    ONNXDeviceCUDA,
			runErr:    errors.New("invalid ONNX output shape"),
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			creations := 0
			client := &localEmbeddingClient{
				session:     &scriptedEmbeddingSession{errors: []error{tc.runErr}},
				execution:   tc.execution,
				device:      tc.device,
				accelerator: ONNXDeviceCUDA,
				newSession: func(ONNXExecutionConfig) (embeddingSession, ONNXDevice, error) {
					creations++
					return &scriptedEmbeddingSession{}, ONNXDeviceCPU, nil
				},
			}
			if err := client.runONNX(nil, []ort.Value{nil}); !errors.Is(err, tc.runErr) {
				t.Fatalf("error = %v, want %v", err, tc.runErr)
			}
			if creations != 0 {
				t.Fatalf("created %d fallback sessions, want 0", creations)
			}
		})
	}
}

func TestExplicitCPUEmbeddingNeverProbesAccelerator(t *testing.T) {
	creations := 0
	client := &localEmbeddingClient{
		session:   &scriptedEmbeddingSession{},
		execution: ONNXExecutionConfig{Device: ONNXDeviceCPU},
		device:    ONNXDeviceCPU,
		newSession: func(ONNXExecutionConfig) (embeddingSession, ONNXDevice, error) {
			creations++
			return &scriptedEmbeddingSession{}, ONNXDeviceCUDA, nil
		},
	}
	if err := client.runONNX(nil, []ort.Value{nil}); err != nil {
		t.Fatal(err)
	}
	if creations != 0 {
		t.Fatalf("explicit CPU created %d accelerator sessions", creations)
	}
}

func TestConcurrentAutoFallbackCreatesOneCPUSession(t *testing.T) {
	gpu := &scriptedEmbeddingSession{errors: []error{errors.New("CUDA resource allocation failed")}}
	cpu := &scriptedEmbeddingSession{}
	creations := 0
	client := &localEmbeddingClient{
		session:     gpu,
		execution:   ONNXExecutionConfig{Device: ONNXDeviceAuto},
		device:      ONNXDeviceCUDA,
		accelerator: ONNXDeviceCUDA,
		newSession: func(cfg ONNXExecutionConfig) (embeddingSession, ONNXDevice, error) {
			creations++
			if cfg.Device != ONNXDeviceCPU {
				return nil, "", fmt.Errorf("unexpected device %q", cfg.Device)
			}
			return cpu, ONNXDeviceCPU, nil
		},
	}

	const callers = 16
	start := make(chan struct{})
	errs := make(chan error, callers)
	var wg sync.WaitGroup
	for i := 0; i < callers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			<-start
			errs <- client.runONNX(nil, []ort.Value{nil})
		}()
	}
	close(start)
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Errorf("concurrent call failed: %v", err)
		}
	}
	if creations != 1 {
		t.Fatalf("CPU fallback sessions = %d, want 1", creations)
	}
	if client.device != ONNXDeviceCPU {
		t.Fatalf("device = %s, want CPU", client.device)
	}
}

func TestConcurrentAutoRecoveryCreatesOneAcceleratorSession(t *testing.T) {
	cpu := &scriptedEmbeddingSession{}
	gpu := &scriptedEmbeddingSession{}
	creations := 0
	client := &localEmbeddingClient{
		session:     cpu,
		execution:   ONNXExecutionConfig{Device: ONNXDeviceAuto},
		device:      ONNXDeviceCPU,
		accelerator: ONNXDeviceCUDA,
		newSession: func(cfg ONNXExecutionConfig) (embeddingSession, ONNXDevice, error) {
			creations++
			if cfg.Device != ONNXDeviceCUDA {
				return nil, "", fmt.Errorf("unexpected device %q", cfg.Device)
			}
			return gpu, ONNXDeviceCUDA, nil
		},
	}

	const callers = 16
	start := make(chan struct{})
	errs := make(chan error, callers)
	var wg sync.WaitGroup
	for i := 0; i < callers; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			<-start
			errs <- client.runONNX(nil, []ort.Value{nil})
		}()
	}
	close(start)
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Errorf("concurrent recovery call failed: %v", err)
		}
	}
	if creations != 1 {
		t.Fatalf("accelerator recovery sessions = %d, want 1", creations)
	}
	if client.device != ONNXDeviceCUDA {
		t.Fatalf("device = %s, want CUDA", client.device)
	}
	if _, destroyed := cpu.counts(); destroyed != 1 {
		t.Fatalf("CPU session destroyed %d times, want 1", destroyed)
	}
}

func TestAcceleratorResourceErrorClassification(t *testing.T) {
	for _, message := range []string{
		"Failed to allocate memory for requested buffer",
		"CUDA_ERROR_OUT_OF_MEMORY",
		"CUBLAS_STATUS_ALLOC_FAILED",
		"CUDNN_STATUS_ALLOC_FAILED",
		"bfc_arena.cc: AllocateRawInternal",
	} {
		if !isAcceleratorResourceError(errors.New(message)) {
			t.Errorf("resource error not recognized: %q", message)
		}
	}
	if isAcceleratorResourceError(errors.New("invalid ONNX output shape")) {
		t.Fatal("non-resource ONNX error was classified as an accelerator resource failure")
	}
}
