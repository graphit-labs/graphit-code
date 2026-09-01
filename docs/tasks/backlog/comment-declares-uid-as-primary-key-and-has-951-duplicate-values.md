# Comment declares UID as PRIMARY KEY and has 951 duplicate values — AST indexer produces colliding UID

# `Comment.uid` has 951 duplicate values despite being PRIMARY KEY

Discovered on 2026-08-21 when writing the export icebug (`docs/tasks/hub-on-s3-icebug-and-lancedb.md`). It's not an export problem — it's a graph problem.

## O fato, medido

```
CALL table_info('Comment') -> uid | primary key: true | STRING

MATCH (c:Comment) WITH c.uid AS u, count(c) AS n WHERE n > 1
RETURN count(u) AS chaves_duplicadas, max(n) AS pior_caso
-> 951 | 2
```

**951 values of `uid` appear twice each**, that is, 1,902 nodes of `Comment` share 951 UIDs, in a column declared as the primary key. The engine is not enforcing the declaration — probably because the load is made by `COPY FROM`, which does not verify.

Example of observed colliding UID:
```
cmd/graphit-grammar-pack/main.go::-platform darwin/arm64:path/to/lib.dylib \
```

The format suggests that the UID of a `Comment` is composed of path + content, and that two identical comments in the same file (or whose content is truncated in the same way) collide.

## Why does it matter

1. **Anything that treats `uid` as unique node identity is wrong.** The export icebug almost attached an edge to the wrong twin because of this — it was saved by a check that turned into a fatal error, and then started to key by `offset(id(n))`.
2. **`MATCH (c:Comment) WHERE c.uid = 'x'` may return two nodes** where the caller expects one.
3. If the intention was that `uid` was unique, there is comment being lost or duplicated in indexing.

## Where to investigate

How the `uid` of an `Comment` is constructed — probably in `internal/ast/`, in the path that transposes the parse result into table rows. Search the UID composition for entities whose "name" is the content itself (`Comment`, `Value`, `Text`, `AttributeValue` — the four labels named by the content).

It is worth checking if the other three also collide:
```
MATCH (n:Value) WITH n.uid AS u, count(n) AS c WHERE c > 1 RETURN count(u)
MATCH (n:Text) WITH n.uid AS u, count(n) AS c WHERE c > 1 RETURN count(u)
MATCH (n:AttributeValue) WITH n.uid AS u, count(n) AS c WHERE c > 1 RETURN count(u)
```

## Possible outputs

1. **Include the line in the UID** (`line_number`), if not already — two identical comments in the same file are on different lines, so this resolves by construction. 2. **Check for truncation**: If the UID cuts content to a fixed size, two long comments with the same prefix collide. In this case, hash the full content. 3. **Accept and document** that `uid` is not unique to content-named labels — but then everything that uses `uid` as identity needs to know.

Preference of those who registered: a **1**, if the line is not in the UID today.

## How to know it worked

```
MATCH (c:Comment) WITH c.uid AS u, count(c) AS n WHERE n > 1 RETURN count(u)
-> 0
```

And the same zero for `Value`, `Text` and `AttributeValue`, after a full reindex.
