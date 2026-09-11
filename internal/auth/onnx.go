package auth

import (
	"fmt"
	"runtime"
	"strings"
)

// ONNXDevice selects the execution provider for a local ONNX-backed AI service.
type ONNXDevice string

const (
	DefaultLocalProviderName = "local"

	ONNXDeviceAuto   ONNXDevice = "auto"
	ONNXDeviceCPU    ONNXDevice = "cpu"
	ONNXDeviceCUDA   ONNXDevice = "cuda"
	ONNXDeviceCoreML ONNXDevice = "coreml"
)

// ONNXExecutionConfig belongs to an AI service whose mode is local. Model manifests remain
// hardware-neutral so the same artifact can execute on any supported device.
type ONNXExecutionConfig struct {
	Device   ONNXDevice `json:"device"`
	DeviceID int        `json:"device_id"`
}

func DefaultONNXExecutionConfig() ONNXExecutionConfig {
	return ONNXExecutionConfig{Device: ONNXDeviceAuto, DeviceID: 0}
}

// DefaultLocalProvider is the persisted provider used whenever no account profile is active.
// Keep independent ONNX values for each service so callers can safely customize either one.
func DefaultLocalProvider() Provider {
	embedding := DefaultONNXExecutionConfig()
	rerank := DefaultONNXExecutionConfig()
	return Provider{
		Name:  DefaultLocalProviderName,
		Type:  ProviderLocal,
		Local: &LocalConfig{},
		AI: AIConfig{
			Embedding: AIServiceConfig{Mode: ServiceLocal, ONNX: &embedding},
			Rerank:    AIServiceConfig{Mode: ServiceLocal, ONNX: &rerank},
		},
	}
}

// ValidateONNXExecution applies the same platform-independent contract used when creating an
// ONNX Runtime session. An empty device is normalized by callers before validation.
func ValidateONNXExecution(cfg ONNXExecutionConfig, goos string) error {
	if cfg.DeviceID < 0 {
		return fmt.Errorf("device_id must be non-negative, got %d", cfg.DeviceID)
	}
	device := ONNXDevice(strings.ToLower(strings.TrimSpace(string(cfg.Device))))
	switch device {
	case ONNXDeviceAuto, ONNXDeviceCPU, ONNXDeviceCUDA:
		if goos == "darwin" && device == ONNXDeviceAuto && cfg.DeviceID != 0 {
			return fmt.Errorf("device_id must be 0 when auto selects CoreML on macOS")
		}
		return nil
	case ONNXDeviceCoreML:
		if goos != "darwin" {
			return fmt.Errorf("coreml is only valid on macOS")
		}
		if cfg.DeviceID != 0 {
			return fmt.Errorf("CoreML requires device_id 0")
		}
		return nil
	default:
		return fmt.Errorf("unknown device %q (expected auto, cpu, cuda, or coreml)", cfg.Device)
	}
}

func validateCurrentPlatformONNXExecution(cfg ONNXExecutionConfig) error {
	return ValidateONNXExecution(cfg, runtime.GOOS)
}
