---
title: Native storage object locations
description: Current S3 object namespaces for native project, user and Hub stores.
type: specification
updated: 2026-10-12
references:
  - target: {type: task, id: tsk-c027}
    relation: implements
  - target: {type: task, id: tsk-c73a}
    relation: derived_from
  - target: {type: task, id: tsk-1dd5}
    relation: consumer_boundary
  - target: {type: task, id: tsk-4208}
    relation: consumer_boundary
---

# Native storage object locations

`graphit_data::native_storage_uri::object_location` builds an object key and S3
URI from explicit bucket, provider prefix, typed scope and relative object name.
It performs no I/O, authentication, credential selection or dataset creation.
The caller supplies actual admitted identities and separately obtains permission
and storage credentials before using the native SDK.

| Scope | Root without provider prefix or object name |
| --- | --- |
| PROJECT Task, Memory, Knowledge, AST | `v1/projects/{project_id}/corpora/{module}` |
| USER Memory | `v1/installations/{installation_id}/users/{identity_key}/corpora/memory` |
| Global Hub registry or rules | `v1/hub/global/{registry\|rules}` |
| Project Hub metadata or events | `v1/projects/{project_id}/hub/{metadata\|events}` |

`identity_key` is the existing Core `UserIdentity::namespace_key()` for the exact
issuer/subject pair. Case and literal bytes remain significant. The URI contains
the namespace hash rather than the literal identity. IDs and keys are data;
PROJECT, USER and global Hub authorization remain separate.

Native dataset callers append their actual dataset name, such as `index.lance`.
They pass the encoded branch and observed dataset version separately to the SDK.
The helper does not translate a local filesystem path or old store layout.
Existing [Hub release and registry keys](remote_artifact_storage.md) retain their
own artifact identity contract.

An empty provider prefix is valid. One trailing slash is a delimiter; other
components must be nonempty, relative and free of standalone `.` or `..`,
backslashes and control bytes. Object names follow the same component rules and
must be nonempty. `url::Url::path_segments_mut()` encodes literal Unicode,
percent, query and fragment characters instead of interpreting them as a URI.
The SDK URI parser must recover the exact object key.

The full UTF-8 key, including prefix, root, delimiters and object, must fit the
[S3 1024-byte limit](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-keys.html).
Invalid input returns `io::ErrorKind::InvalidInput` with no echoed identity or
credential. Future SDK object names are checked when known; this helper reserves
no artificial allowance for them. `NativeObjectLocation` exposes borrowed key
and URI getters or moves both values through `into_parts()`.

The bucket is a 3–63-byte lowercase S3 authority with letters, digits, hyphens
or periods and alphanumeric ends. Userinfo, ports, URI delimiters and adjacent
periods are rejected; the helper does not rewrite the bucket's case.

This producer does not qualify remote PROJECT/USER mounts, Broker grants,
STS/S3 operations or the global Hub and metadata/event codecs. Those consumers
must use the actual namespace and their own native admission and SDK contracts.

## Affected verification

`cargo test --locked --offline -p graphit-data --test native_storage_uri` exercises
scope isolation, literal identity separation, SDK URI roundtrips, path/authority
rejection and exact 1024/1025-byte keys including a multibyte provider prefix.
The three focused tests passed on Linux using the existing `object_store` URL
path parser. This checks decoding without enabling or constructing an AWS
client. No S3 network operation is required by this pure location producer.
