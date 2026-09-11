package hub

import (
	"encoding/binary"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/klauspost/compress/zstd"
)

func installGrammarArchive(archivePath, projectDir, dotDir string) error {
	archive, err := ast.ReadGrammarArchive(archivePath)
	if err != nil {
		return fmt.Errorf("reading grammar archive: %w", err)
	}

	baseName := filepath.Base(archivePath)
	nameStem := strings.TrimSuffix(baseName, ".grammar")

	var grammarDir string
	var targetName string
	var needsExec bool

	if strings.HasPrefix(nameStem, "tree-sitter-") {
		if dotDir == "" {
			grammarDir = filepath.Join(projectDir, "grammars", "treesitter")
		} else {
			grammarDir = filepath.Join(projectDir, dotDir, "grammars", "treesitter")
		}
		ext := sharedLibExt()
		targetName = nameStem + ext
	} else if strings.HasPrefix(nameStem, "antlr-") {
		if dotDir == "" {
			grammarDir = filepath.Join(projectDir, "grammars", "antlr")
		} else {
			grammarDir = filepath.Join(projectDir, dotDir, "grammars", "antlr")
		}
		langName := strings.TrimPrefix(nameStem, "antlr-")
		targetName = "antlr-sidecar-" + langName
		if runtime.GOOS == "windows" {
			targetName += ".exe"
		}
		needsExec = true
	} else {
		return fmt.Errorf("unknown grammar archive type: %s", baseName)
	}

	if err := os.MkdirAll(grammarDir, 0o755); err != nil {
		return fmt.Errorf("creating grammar dir: %w", err)
	}

	platformData, err := extractPlatformData(archive, archivePath)
	if err != nil {
		return fmt.Errorf("extracting platform data from %s: %w", baseName, err)
	}

	targetPath := filepath.Join(grammarDir, targetName)
	perm := os.FileMode(0o644)
	if needsExec {
		perm = 0o755
	}

	if err := os.WriteFile(targetPath, platformData, perm); err != nil {
		return fmt.Errorf("writing grammar binary: %w", err)
	}

	ast.InvalidateQueryCaches()

	return nil
}

func extractPlatformData(archive *ast.GrammarArchive, archivePath string) ([]byte, error) {
	goos := runtime.GOOS
	goarch := runtime.GOARCH

	idx := -1
	for i, p := range archive.Platforms {
		if p.OS == goos && p.Arch == goarch {
			idx = i
			break
		}
	}
	if idx == -1 {
		return nil, fmt.Errorf("no binary for platform %s/%s in archive", goos, goarch)
	}

	f, err := os.Open(archivePath)
	if err != nil {
		return nil, fmt.Errorf("open archive: %w", err)
	}
	defer f.Close()

	entryOffset := int64(16 + idx*120)
	if _, err := f.Seek(entryOffset, io.SeekStart); err != nil {
		return nil, fmt.Errorf("seek to entry: %w", err)
	}

	var skip [96]byte
	if _, err := io.ReadFull(f, skip[:]); err != nil {
		return nil, fmt.Errorf("skip entry fields: %w", err)
	}

	var dataOffset, compressedSize, originalSize uint64
	if err := binary.Read(f, binary.LittleEndian, &dataOffset); err != nil {
		return nil, fmt.Errorf("read offset: %w", err)
	}
	if err := binary.Read(f, binary.LittleEndian, &compressedSize); err != nil {
		return nil, fmt.Errorf("read compressed size: %w", err)
	}
	if err := binary.Read(f, binary.LittleEndian, &originalSize); err != nil {
		return nil, fmt.Errorf("read original size: %w", err)
	}

	if _, err := f.Seek(int64(dataOffset), io.SeekStart); err != nil {
		return nil, fmt.Errorf("seek to data: %w", err)
	}

	compressed := make([]byte, compressedSize)
	if _, err := io.ReadFull(f, compressed); err != nil {
		return nil, fmt.Errorf("read compressed data: %w", err)
	}

	decoder, err := zstd.NewReader(nil)
	if err != nil {
		return nil, fmt.Errorf("create zstd decoder: %w", err)
	}
	defer decoder.Close()

	decompressed, err := decoder.DecodeAll(compressed, nil)
	if err != nil {
		return nil, fmt.Errorf("zstd decompress: %w", err)
	}

	if uint64(len(decompressed)) != originalSize {
		return nil, fmt.Errorf("size mismatch: decompressed %d, expected %d", len(decompressed), originalSize)
	}

	return decompressed, nil
}

func uninstallGrammarFiles(cloneDir, projectDir, dotDir string) {
	entries, _ := os.ReadDir(cloneDir)
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".grammar") {
			continue
		}
		nameStem := strings.TrimSuffix(e.Name(), ".grammar")

		if strings.HasPrefix(nameStem, "tree-sitter-") {
			ext := sharedLibExt()
			var target string
			if dotDir == "" {
				target = filepath.Join(projectDir, "grammars", "treesitter", nameStem+ext)
			} else {
				target = filepath.Join(projectDir, dotDir, "grammars", "treesitter", nameStem+ext)
			}
			_ = os.Remove(target)
		} else if strings.HasPrefix(nameStem, "antlr-") {
			langName := strings.TrimPrefix(nameStem, "antlr-")
			targetName := "antlr-sidecar-" + langName
			if runtime.GOOS == "windows" {
				targetName += ".exe"
			}
			var target string
			if dotDir == "" {
				target = filepath.Join(projectDir, "grammars", "antlr", targetName)
			} else {
				target = filepath.Join(projectDir, dotDir, "grammars", "antlr", targetName)
			}
			_ = os.Remove(target)
		}
	}

	ast.InvalidateQueryCaches()
}

func sharedLibExt() string {
	switch runtime.GOOS {
	case "darwin":
		return ".dylib"
	case "windows":
		return ".dll"
	default:
		return ".so"
	}
}
