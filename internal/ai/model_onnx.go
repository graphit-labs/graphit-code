package ai

import (
	"fmt"
	"strings"

	ort "github.com/yalue/onnxruntime_go"
)

type onnxInputBinding struct {
	Name string
	Role string
}

type onnxTextContract struct {
	Inputs      []onnxInputBinding
	Output      string
	OutputShape ort.Shape
}

func inspectONNXTextContract(manifest ModelManifest, modelPath string) (onnxTextContract, error) {
	inputs, outputs, err := ort.GetInputOutputInfo(modelPath)
	if err != nil {
		return onnxTextContract{}, fmt.Errorf("inspect ONNX signature for model %q at %s: %w", manifest.ID, modelPath, err)
	}
	if len(inputs) == 0 || len(outputs) == 0 {
		return onnxTextContract{}, fmt.Errorf("model %q must declare at least one ONNX input and output", manifest.ID)
	}

	contract := onnxTextContract{Inputs: make([]onnxInputBinding, 0, len(inputs))}
	seenRoles := map[string]bool{}
	for _, input := range inputs {
		if input.OrtValueType != ort.ONNXTypeTensor || input.DataType != ort.TensorElementDataTypeInt64 {
			return onnxTextContract{}, fmt.Errorf("model %q input %q must be an int64 tensor", manifest.ID, input.Name)
		}
		role := resolveInputRole(input.Name, manifest.Inference.Inputs)
		if role == "" {
			return onnxTextContract{}, fmt.Errorf("model %q wants unsupported ONNX input %q; bind it to input_ids, attention_mask, or token_type_ids", manifest.ID, input.Name)
		}
		if seenRoles[role] {
			return onnxTextContract{}, fmt.Errorf("model %q maps multiple ONNX inputs to role %q", manifest.ID, role)
		}
		seenRoles[role] = true
		contract.Inputs = append(contract.Inputs, onnxInputBinding{Name: input.Name, Role: role})
	}
	for _, required := range []string{"input_ids", "attention_mask"} {
		if !seenRoles[required] {
			return onnxTextContract{}, fmt.Errorf("model %q has no ONNX input bound to %q", manifest.ID, required)
		}
	}
	for role, name := range manifest.Inference.Inputs {
		if role != "input_ids" && role != "attention_mask" && role != "token_type_ids" {
			return onnxTextContract{}, fmt.Errorf("model %q declares unsupported logical input role %q", manifest.ID, role)
		}
		found := false
		for _, input := range inputs {
			if input.Name == name {
				found = true
				break
			}
		}
		if !found {
			return onnxTextContract{}, fmt.Errorf("model %q binds %q to missing ONNX input %q", manifest.ID, role, name)
		}
	}

	output, err := selectONNXOutput(manifest, outputs)
	if err != nil {
		return onnxTextContract{}, err
	}
	if output.OrtValueType != ort.ONNXTypeTensor || output.DataType != ort.TensorElementDataTypeFloat {
		return onnxTextContract{}, fmt.Errorf("model %q output %q must be a float32 tensor", manifest.ID, output.Name)
	}
	contract.Output = output.Name
	contract.OutputShape = output.Dimensions.Clone()

	if manifest.Task == ModelTaskEmbedding {
		wantRank := 2
		if manifest.Inference.Pooling == "mean" || manifest.Inference.Pooling == "cls" {
			wantRank = 3
		}
		if len(output.Dimensions) != wantRank {
			return onnxTextContract{}, fmt.Errorf("embedding model %q output %q has rank %d, but pooling %q requires rank %d", manifest.ID, output.Name, len(output.Dimensions), manifest.Inference.Pooling, wantRank)
		}
		last := output.Dimensions[len(output.Dimensions)-1]
		if last > 0 && int(last) != manifest.Inference.Dimensions {
			return onnxTextContract{}, fmt.Errorf("embedding model %q output dimension is %d, manifest declares %d", manifest.ID, last, manifest.Inference.Dimensions)
		}
	} else if manifest.Task == ModelTaskRerank && len(output.Dimensions) != 1 && len(output.Dimensions) != 2 {
		return onnxTextContract{}, fmt.Errorf("rerank model %q output %q has rank %d, want rank 1 or 2", manifest.ID, output.Name, len(output.Dimensions))
	}
	return contract, nil
}

func resolveInputRole(actual string, overrides map[string]string) string {
	for role, name := range overrides {
		if name == actual {
			return role
		}
	}
	normalized := strings.ToLower(strings.TrimSpace(actual))
	switch normalized {
	case "input_ids", "attention_mask", "token_type_ids":
		return normalized
	default:
		return ""
	}
}

func selectONNXOutput(manifest ModelManifest, outputs []ort.InputOutputInfo) (ort.InputOutputInfo, error) {
	if selected := strings.TrimSpace(manifest.Inference.Output); selected != "" {
		for _, output := range outputs {
			if output.Name == selected {
				return output, nil
			}
		}
		return ort.InputOutputInfo{}, fmt.Errorf("model %q selects missing ONNX output %q", manifest.ID, selected)
	}
	if len(outputs) == 1 {
		return outputs[0], nil
	}
	preferred := []string{"sentence_embedding", "embeddings", "last_hidden_state"}
	if manifest.Task == ModelTaskRerank {
		preferred = []string{"logits", "scores", "score"}
	}
	for _, name := range preferred {
		for _, output := range outputs {
			if strings.EqualFold(output.Name, name) {
				return output, nil
			}
		}
	}
	return ort.InputOutputInfo{}, fmt.Errorf("model %q has multiple ONNX outputs; inference.output is required", manifest.ID)
}
