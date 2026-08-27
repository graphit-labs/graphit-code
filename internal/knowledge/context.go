package knowledge

import (
	"context"

	"github.com/graphit-labs/graphit-code/internal/wiki"
)

// An imported knowledge context arrives COMPILED and is never recompiled here.
//
// The producer ran the generator once and published what came out of it: the pages,
// the chunk shards and the embedding vectors, all content-addressed. The consumer's
// only job is to build its own search index from those shards.
//
// This used to work the other way round — the publisher pushed its `docs/` tree
// alongside the compiled wiki, and the consumer threw the compiled half away and ran
// the whole pipeline over the sources. That paid for the embedding model a second
// time, on text whose vectors had just been downloaded, and it meant the same
// documentation existed in two shapes depending on which install path created it.
//
// So: the source of a published context does not travel, and the consumer does not
// have a generator in its path at all.

// ResetContextWiki clears a context's stored wiki and returns the directory a fresh
// copy should be placed in.
//
// It clears rather than merges because the extraction that follows is additive: a
// page the producer deleted would otherwise survive in every consumer forever,
// answering searches with documentation that no longer exists upstream.
func ResetContextWiki(name string) (string, error) {
	return wiki.ResetDir(ContextWriteDir(name))
}

// IndexContextWiki builds a context's search index from the shards it carries, and
// reports how many chunks were indexed.
//
// Zero is not an error and is not success either: it means what arrived carried no
// usable shards — an artifact published empty, or one published by a version that
// still shipped only sources. The caller has to say so, because an empty index
// answers every query with "no results" for a reason that has nothing to do with the
// query.
func IndexContextWiki(ctx context.Context, name string) (int, error) {
	return wiki.BuildDBFromCache(ctx, ContextWriteDir(name))
}
