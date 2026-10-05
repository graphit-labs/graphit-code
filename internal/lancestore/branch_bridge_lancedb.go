//go:build lancedb

package lancestore

/*
#include <stdlib.h>
#include <stdbool.h>
#include <stdint.h>

typedef struct SimpleResult {
  bool SUCCESS;
  char *ERROR_MESSAGE;
} SimpleResult;

struct SimpleResult *simple_lancedb_list_branches(const char *table_uri,
                                                  const char *storage_options_json,
                                                  char **branches_json);
struct SimpleResult *simple_lancedb_delete_branch(const char *table_uri,
                                                  const char *branch,
                                                  bool force,
                                                  const char *storage_options_json);
void simple_lancedb_result_free(struct SimpleResult *result);
void simple_lancedb_free_string(char *s);
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"unsafe"
)

// listBranchesNative lists the native Lance branches of the table dataset at tableURI.
func listBranchesNative(tableURI string, storageOptions map[string]string) (map[string]BranchInfo, error) {
	optionsJSON, err := json.Marshal(nativeStorageOptions(storageOptions))
	if err != nil {
		return nil, err
	}
	cURI := C.CString(tableURI)
	cOptions := C.CString(string(optionsJSON))
	defer C.free(unsafe.Pointer(cURI))
	defer C.free(unsafe.Pointer(cOptions))
	var out *C.char
	result := C.simple_lancedb_list_branches(cURI, cOptions, &out) //nolint:gocritic // cgo's pointer check expands &out into a self-comparison
	defer C.simple_lancedb_result_free(result)
	if !result.SUCCESS {
		if result.ERROR_MESSAGE != nil {
			return nil, fmt.Errorf("%s", C.GoString(result.ERROR_MESSAGE))
		}
		return nil, ErrNoBranches
	}
	defer C.simple_lancedb_free_string(out)
	branches := map[string]BranchInfo{}
	if err := json.Unmarshal([]byte(C.GoString(out)), &branches); err != nil {
		return nil, fmt.Errorf("lancestore: decoding branches: %w", err)
	}
	return branches, nil
}

// deleteBranchNative deletes a native Lance branch of the table dataset at tableURI.
// force also removes a branch dataset whose metadata is missing.
func deleteBranchNative(tableURI, branch string, force bool, storageOptions map[string]string) error {
	optionsJSON, err := json.Marshal(nativeStorageOptions(storageOptions))
	if err != nil {
		return err
	}
	cURI := C.CString(tableURI)
	cBranch := C.CString(branch)
	cOptions := C.CString(string(optionsJSON))
	defer C.free(unsafe.Pointer(cURI))
	defer C.free(unsafe.Pointer(cBranch))
	defer C.free(unsafe.Pointer(cOptions))
	result := C.simple_lancedb_delete_branch(cURI, cBranch, C.bool(force), cOptions)
	defer C.simple_lancedb_result_free(result)
	if result.SUCCESS {
		return nil
	}
	if result.ERROR_MESSAGE != nil {
		return fmt.Errorf("%s", C.GoString(result.ERROR_MESSAGE))
	}
	return ErrNoBranches
}

// nativeStorageOptions adds the canonical aws_* spelling the Lance object store reads,
// as cloneTableNative does.
func nativeStorageOptions(storageOptions map[string]string) map[string]string {
	out := make(map[string]string, len(storageOptions)*2)
	for key, value := range storageOptions {
		out[key] = value
	}
	for legacy, canonical := range map[string]string{
		storageKeyRegion:        "aws_region",
		storageKeyEndpoint:      "aws_endpoint",
		storageKeyAccessKeyID:   "aws_access_key_id",
		storageKeySecretKey:     "aws_secret_access_key",
		storageKeySessionToken:  "aws_session_token",
		storageKeyVirtualHosted: "aws_virtual_hosted_style_request",
	} {
		if value, ok := out[legacy]; ok {
			out[canonical] = value
		}
	}
	return out
}
