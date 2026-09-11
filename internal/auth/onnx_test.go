package auth

import (
	"strings"
	"testing"
)

func TestValidateProviderScopesONNXExecutionToLocalServices(t *testing.T) {
	execution := DefaultONNXExecutionConfig()
	for _, mode := range []ServiceMode{ServiceDirect, ServiceBroker, ServiceDisabled} {
		service := AIServiceConfig{Mode: mode, ONNX: &execution}
		if err := validateAIService(service, true); err == nil || !strings.Contains(err.Error(), "ONNX") {
			t.Fatalf("mode %q: error = %v", mode, err)
		}
	}
}

func TestValidateProviderRejectsInvalidLocalONNXExecution(t *testing.T) {
	execution := ONNXExecutionConfig{Device: "metal", DeviceID: 0}
	err := validateAIService(AIServiceConfig{Mode: ServiceLocal, ONNX: &execution}, true)
	if err == nil || !strings.Contains(err.Error(), "unknown device") {
		t.Fatalf("error = %v", err)
	}
}
