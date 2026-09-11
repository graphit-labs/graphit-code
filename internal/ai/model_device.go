package ai

import (
	"errors"
	"fmt"
	"runtime"
	"strconv"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/auth"
	ort "github.com/yalue/onnxruntime_go"
)

type ONNXDevice = auth.ONNXDevice

const (
	ONNXDeviceAuto   = auth.ONNXDeviceAuto
	ONNXDeviceCPU    = auth.ONNXDeviceCPU
	ONNXDeviceCUDA   = auth.ONNXDeviceCUDA
	ONNXDeviceCoreML = auth.ONNXDeviceCoreML
)

type ONNXExecutionConfig = auth.ONNXExecutionConfig

// ValidateConfiguredLocalONNXDevices performs the startup checks that can be completed without
// loading model artifacts. Auto and CPU are always safe to defer: auto owns a CPU fallback and CPU
// registers no accelerator. An explicitly requested accelerator is different; its provider must
// load before the daemon advertises a healthy local service.
func ValidateConfiguredLocalONNXDevices(tasks ...ModelTask) error {
	for _, task := range tasks {
		if !ConfiguredServiceIsLocal(task) {
			continue
		}
		execution, err := configuredONNXExecution(task)
		if err != nil {
			return fmt.Errorf("configured %s ONNX execution: %w", task, err)
		}
		if execution.Device != ONNXDeviceCUDA && execution.Device != ONNXDeviceCoreML {
			continue
		}
		if err := initONNXRuntime(); err != nil {
			return fmt.Errorf("configured %s ONNX device %s: initialize runtime: %w", task, execution.Device, err)
		}
		opts, err := ort.NewSessionOptions()
		if err != nil {
			return fmt.Errorf("configured %s ONNX device %s: create session options: %w", task, execution.Device, err)
		}
		providerErr := appendONNXExecutionProvider(opts, execution.Device, execution.DeviceID)
		_ = opts.Destroy()
		if providerErr != nil {
			return fmt.Errorf("configured %s ONNX device %s (device_id=%d): %w", task, execution.Device, execution.DeviceID, providerErr)
		}
	}
	return nil
}

func configuredONNXExecution(task ModelTask) (ONNXExecutionConfig, error) {
	snapshot, active := activeAuthSnapshot()
	if !active {
		return ONNXExecutionConfig{}, errors.New("resolve active or default local provider")
	}
	var service auth.AIServiceConfig
	switch task {
	case ModelTaskEmbedding:
		service = snapshot.Provider.AI.Embedding
	case ModelTaskRerank:
		service = snapshot.Provider.AI.Rerank
	default:
		return ONNXExecutionConfig{}, fmt.Errorf("ONNX execution is unsupported for model task %q", task)
	}
	mode := service.Mode
	if mode == "" {
		mode = auth.ServiceLocal
	}
	if mode != auth.ServiceLocal {
		return ONNXExecutionConfig{}, fmt.Errorf("%s service mode %q does not use local ONNX execution", task, mode)
	}
	result := auth.DefaultONNXExecutionConfig()
	if service.ONNX != nil {
		result = *service.ONNX
	}
	if err := validateONNXExecution(result, runtime.GOOS); err != nil {
		return ONNXExecutionConfig{}, fmt.Errorf("provider %q %s ONNX execution: %w", snapshot.Provider.Name, task, err)
	}
	return result, nil
}

// ConfiguredONNXExecution returns the active provider service settings. When no profile is active,
// resolution first materializes and reads the persisted default local provider.
func ConfiguredONNXExecution(task ModelTask) (ONNXExecutionConfig, error) {
	return configuredONNXExecution(task)
}

// ParseONNXExecution validates provider input using the same rules as runtime session creation.
func ParseONNXExecution(device, deviceID string) (ONNXExecutionConfig, error) {
	parsedDevice := ONNXDevice(strings.ToLower(strings.TrimSpace(device)))
	if parsedDevice == "" {
		parsedDevice = ONNXDeviceAuto
	}
	parsedID := 0
	if raw := strings.TrimSpace(deviceID); raw != "" {
		value, err := strconv.Atoi(raw)
		if err != nil {
			return ONNXExecutionConfig{}, fmt.Errorf("device_id must be a non-negative integer, got %q", raw)
		}
		parsedID = value
	}
	result := ONNXExecutionConfig{Device: parsedDevice, DeviceID: parsedID}
	if err := validateONNXExecution(result, runtime.GOOS); err != nil {
		return ONNXExecutionConfig{}, err
	}
	return result, nil
}

func validateONNXExecution(cfg ONNXExecutionConfig, goos string) error {
	return auth.ValidateONNXExecution(cfg, goos)
}

func onnxDeviceCandidates(cfg ONNXExecutionConfig, goos string) ([]ONNXDevice, error) {
	if err := validateONNXExecution(cfg, goos); err != nil {
		return nil, err
	}
	if cfg.Device != ONNXDeviceAuto {
		return []ONNXDevice{cfg.Device}, nil
	}
	if goos == "darwin" {
		return []ONNXDevice{ONNXDeviceCoreML, ONNXDeviceCPU}, nil
	}
	return []ONNXDevice{ONNXDeviceCUDA, ONNXDeviceCPU}, nil
}

func tryONNXDevices(cfg ONNXExecutionConfig, goos string, attempt func(ONNXDevice, int) error) (ONNXDevice, error) {
	candidates, err := onnxDeviceCandidates(cfg, goos)
	if err != nil {
		return "", err
	}
	var failures []error
	for _, candidate := range candidates {
		if err := attempt(candidate, cfg.DeviceID); err == nil {
			return candidate, nil
		} else {
			failures = append(failures, fmt.Errorf("%s: %w", candidate, err))
		}
	}
	return "", fmt.Errorf("could not initialize ONNX device %s (device_id=%d): %w", cfg.Device, cfg.DeviceID, errors.Join(failures...))
}

func appendONNXExecutionProvider(opts *ort.SessionOptions, device ONNXDevice, deviceID int) error {
	switch device {
	case ONNXDeviceCPU:
		return nil
	case ONNXDeviceCUDA:
		cudaOptions, err := ort.NewCUDAProviderOptions()
		if err != nil {
			return fmt.Errorf("create CUDA provider options: %w", err)
		}
		defer func() { _ = cudaOptions.Destroy() }()
		if err := cudaOptions.Update(map[string]string{"device_id": strconv.Itoa(deviceID)}); err != nil {
			return fmt.Errorf("select CUDA device %d: %w", deviceID, err)
		}
		if err := opts.AppendExecutionProviderCUDA(cudaOptions); err != nil {
			return fmt.Errorf("enable CUDA provider: %w", err)
		}
		return nil
	case ONNXDeviceCoreML:
		if err := opts.AppendExecutionProviderCoreMLV2(map[string]string{}); err != nil {
			return fmt.Errorf("enable CoreML provider: %w", err)
		}
		return nil
	default:
		return fmt.Errorf("unsupported resolved ONNX device %q", device)
	}
}

func newDynamicONNXSession(
	modelPath string,
	inputNames, outputNames []string,
	execution ONNXExecutionConfig,
	tune func(*ort.SessionOptions) error,
) (*ort.DynamicAdvancedSession, ONNXDevice, error) {
	var session *ort.DynamicAdvancedSession
	selected, err := tryONNXDevices(execution, runtime.GOOS, func(device ONNXDevice, deviceID int) error {
		opts, err := ort.NewSessionOptions()
		if err != nil {
			return fmt.Errorf("create session options: %w", err)
		}
		defer func() { _ = opts.Destroy() }()
		if tune != nil {
			if err := tune(opts); err != nil {
				return err
			}
		}
		if err := appendONNXExecutionProvider(opts, device, deviceID); err != nil {
			return err
		}
		created, err := ort.NewDynamicAdvancedSession(modelPath, inputNames, outputNames, opts)
		if err != nil {
			return fmt.Errorf("create session: %w", err)
		}
		session = created
		return nil
	})
	if err != nil {
		return nil, "", err
	}
	return session, selected, nil
}
