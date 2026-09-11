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

struct SimpleResult *simple_lancedb_clone_table(const char *target_uri,
                                                const char *target_table_name,
                                                const char *source_uri,
                                                uint64_t source_version,
                                                bool has_source_version,
                                                const char *source_tag,
                                                const char *storage_options_json);
void simple_lancedb_result_free(struct SimpleResult *result);
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"unsafe"
)

func cloneTableNative(targetURI, targetName, sourceURI string, opts CloneOptions, storageOptions map[string]string) error {
	if opts.SourceVersion != nil && opts.SourceTag != "" {
		return fmt.Errorf("source version and source tag are mutually exclusive")
	}
	if storageOptions == nil {
		storageOptions = map[string]string{}
	}
	for legacy, canonical := range map[string]string{
		storageKeyRegion:        "aws_region",
		storageKeyEndpoint:      "aws_endpoint",
		storageKeyAccessKeyID:   "aws_access_key_id",
		storageKeySecretKey:     "aws_secret_access_key",
		storageKeyVirtualHosted: "aws_virtual_hosted_style_request",
	} {
		if value, ok := storageOptions[legacy]; ok {
			storageOptions[canonical] = value
		}
	}
	if !isRemoteURI(targetURI) {
		resolved, err := localFileURI(targetURI)
		if err != nil {
			return fmt.Errorf("resolve clone target: %w", err)
		}
		targetURI = resolved
	}
	optionsJSON, err := json.Marshal(storageOptions)
	if err != nil {
		return err
	}
	cTargetURI := C.CString(targetURI)
	cTargetName := C.CString(targetName)
	cSourceURI := C.CString(sourceURI)
	cOptions := C.CString(string(optionsJSON))
	defer C.free(unsafe.Pointer(cTargetURI))
	defer C.free(unsafe.Pointer(cTargetName))
	defer C.free(unsafe.Pointer(cSourceURI))
	defer C.free(unsafe.Pointer(cOptions))
	var cTag *C.char
	if opts.SourceTag != "" {
		cTag = C.CString(opts.SourceTag)
		defer C.free(unsafe.Pointer(cTag))
	}
	var sourceVersion C.uint64_t
	var hasSourceVersion C.bool
	if opts.SourceVersion != nil {
		sourceVersion = C.uint64_t(*opts.SourceVersion)
		hasSourceVersion = true
	}
	result := C.simple_lancedb_clone_table(cTargetURI, cTargetName, cSourceURI, sourceVersion, hasSourceVersion, cTag, cOptions)
	defer C.simple_lancedb_result_free(result)
	if result.SUCCESS {
		return nil
	}
	if result.ERROR_MESSAGE != nil {
		return fmt.Errorf("%s", C.GoString(result.ERROR_MESSAGE))
	}
	return ErrNoShallowClone
}
