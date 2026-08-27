package memory

import (
	"io/fs"
	"os"
	"path/filepath"
	"strings"
)

// A memory scope's shards travel in its prefix, beside the raw markdown.
//
// The raw markdown is the truth and it must travel: memory is read-AND-write, the
// same prefix is written to by every unit on a team, and a consumer recompiles
// its own wiki from what it pulled. That is the opposite of a knowledge context,
// which arrives compiled and is never recompiled.
//
// The shards travel anyway, and that is the whole point of this file: they are a
// content-addressed cache of chunking and — expensively — of embedding vectors. A
// developer who pulls a colleague's memory should not pay the embedding model again
// for text whose vector is already computed and sitting in the prefix.
//
// Nothing here can corrupt a wiki. A shard is keyed by the content hash of its source
// file, so one that does not match is simply never read: WikiProcessCache.Get compares
// the hash before returning anything. That is what makes the import safe to do blind,
// before the compile, without knowing whether the local sources agree with it.
//
// The mirror lives in a SUBDIRECTORY of the raw directory, which keeps it out of the memory
// scan for free: memorySourceFileNames reads the raw directory non-recursively and skips
// directories, so `.wiki/` is invisible to it.

// shardMirrorDirName is where a scope's shard mirror sits inside its raw directory.
const shardMirrorDirName = ".wiki"

func shardMirrorDir(rawDir string) string {
	return filepath.Join(rawDir, shardMirrorDirName)
}

// ImportShards copies the shards carried by a scope's prefix into its wiki cache,
// before the wiki is compiled.
//
// The copy is ADDITIVE: it never overwrites and never deletes. A local shard is
// either identical to the prefix's — same content hash, same bytes — or it is newer,
// belonging to a memory this developer wrote and has not pushed yet. Mirroring would
// destroy the second case, and there is nothing to gain from it, because a stale
// entry in a content-addressed cache is inert rather than wrong.
func ImportShards(rawDir, wikiDir string) (int, error) {
	if rawDir == "" || wikiDir == "" {
		return 0, nil
	}
	return copyMissing(shardMirrorDir(rawDir), wikiDir)
}

// ExportShards mirrors a scope's compiled shards back into its raw directory, so the next
// commit carries them to everyone else.
//
// This one MIRRORS, and it is safe to: it runs after a successful compile over the
// raw directory, so the local cache covers exactly the memories the prefix holds — plus any
// this unit has added, which belong in the prefix too. Mirroring is what lets a
// deleted memory's shard leave the prefix instead of outliving it forever.
//
// Only the shard tree is mirrored. The compiled pages, the index and the database stay
// local: a consumer builds its own from its own raw markdown, and publishing them
// would put a second, divergent copy of every page in a prefix that many people write
// to — the exact shape of the problem the per-file cache layout exists to avoid.
func ExportShards(rawDir, wikiDir string) error {
	if rawDir == "" || wikiDir == "" {
		return nil
	}
	src := filepath.Join(wikiDir, "shards")
	if _, err := os.Stat(src); err != nil {
		return nil //nolint:nilerr // nothing compiled yet is not a failure
	}
	return mirrorShards(src, filepath.Join(shardMirrorDir(rawDir), "shards"))
}

// copyMissing copies every file under src that is absent from dst, and reports how
// many it copied.
func copyMissing(src, dst string) (int, error) {
	if _, err := os.Stat(src); err != nil {
		return 0, nil //nolint:nilerr // no shards in the prefix is the common case
	}
	copied := 0
	err := filepath.WalkDir(src, func(path string, d fs.DirEntry, walkErr error) error {
		if walkErr != nil || d.IsDir() {
			return nil //nolint:nilerr // an unreadable entry costs a cache miss, nothing more
		}
		rel, relErr := filepath.Rel(src, path)
		if relErr != nil {
			return nil
		}
		target := filepath.Join(dst, rel)
		if _, err := os.Stat(target); err == nil {
			return nil
		}
		data, readErr := os.ReadFile(path)
		if readErr != nil {
			return nil
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return nil
		}
		if os.WriteFile(target, data, 0o644) == nil {
			copied++
		}
		return nil
	})
	return copied, err
}

// mirrorShards makes dst hold exactly what src holds.
//
// Written here rather than reusing paths.SyncCopyDir because this one skips the
// temporary files the shard writer leaves mid-rename: a `.tmp` sibling committed into
// a shared prefix is noise at best, and a half-written shard at worst.
func mirrorShards(src, dst string) error {
	if err := os.MkdirAll(dst, 0o755); err != nil {
		return err
	}

	want := make(map[string]bool)
	if err := filepath.WalkDir(src, func(path string, d fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		if d.IsDir() || strings.HasSuffix(path, ".tmp") {
			return nil
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		want[rel] = true
		target := filepath.Join(dst, rel)
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		if existing, err := os.ReadFile(target); err == nil && string(existing) == string(data) {
			return nil
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		return os.WriteFile(target, data, 0o644)
	}); err != nil {
		return err
	}

	// Drop what the source no longer has, so a deleted memory's shard leaves.
	return filepath.WalkDir(dst, func(path string, d fs.DirEntry, walkErr error) error {
		if walkErr != nil || d.IsDir() {
			return nil //nolint:nilerr
		}
		rel, err := filepath.Rel(dst, path)
		if err != nil || want[rel] {
			return nil //nolint:nilerr
		}
		_ = os.Remove(path)
		return nil
	})
}
