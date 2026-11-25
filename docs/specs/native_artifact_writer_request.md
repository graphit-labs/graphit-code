---
title: Artifact installation and catalogue writes
type: specification
---

# Artifact installation and catalogue writes

Graphit installs package and Hub content using ordinary filesystem paths. The package is the canonical source of built-in content; installed files are the effective installation. `artifact-store.json` records the logical owner and entries with their key, checkout selection, source, destination, kind, mode, installed content hash and optional package origin/version. It stores no directory identities, ancestor handles, permissions snapshots, intents, backups or recovery phases.

`PublicationScope` selects the logical PROJECT, USER or installation and the configured bases. A PROJECT checkout uses `PublicationSlot::ProjectRoot(PathBuf)` so the same artifact key can be installed in different worktrees without changing the project’s shared corpus identity. Installation and USER records use `Scope`. These local installation records can contain runtime-resolved absolute locations; shared configuration and package templates must remain portable.

Publication destinations are absolute Unicode platform paths without parent traversal,
strictly below a configured base and outside the runtime tree. They use native path
components, including ordinary Windows separators, rather than payload-locator syntax.
PROJECT destinations derive from the root returned by `ProjectPaths`; that producer
supplies the canonical platform spelling used by the scope.

Configured roots may be symlinks, junctions or mount points. The operating system resolves them normally. Graphit does not open or compare every ancestor, pin directory identities, deny a different volume, or reject a benign metadata change. The scope is selection data, not filesystem authorization. Authentication and PROJECT/version authorization remain the responsibility of the operation’s caller.

External payload entry names are a separate boundary. Names must be relative, valid Unicode and free of parent traversal. Creation happens inside a fresh temporary output tree, so an external name cannot choose another installation target. An external payload policy may reject entry links; a configured root is not an untrusted payload entry. `CopyPolicy::FollowLinks` supports legitimate file and directory links, with cycles reported by the walker.

## Publication and user edits

Preparation and content copying happen before the catalogue writer lock. The operation then takes a kernel file lock, reloads the current catalogue, and compares the installed content with its recorded hash before replacing or removing it. This prevents cooperating writers from losing another entry and prevents sync from overwriting edited user content. Directory owner, inode, timestamp and mode equality are not prerequisites for an update.

New file output is prepared in the destination’s directory and replaced using normal filesystem replacement. This places the temporary file on the volume required by the replacement operation; the source may be on another volume. Package attributes are applied to newly created output, never changed on existing files merely to make access succeed.

Removing or replacing a directory link detaches the link itself and preserves its source tree. Windows directory links use `remove_dir`; Unix links use `remove_file`. The link type comes from `symlink_metadata`, without following the target. This applies to Link-to-Copy replacement and explicit removal alike.

Directory installations are reproducible operations rather than a transaction spanning every file and the catalogue. After verifying the old installed content, Graphit replaces that directory and records the new installation. There is no backup, rollback, permission-grant journal or automatic replay. Cancellation is checked before the filesystem effect and while preparing data; once an effect is applied, its result is recorded without inventing a fresh authorization context.

The catalogue itself is written through a temporary file, synced and atomically replaced while holding the writer lock. Body/key/tree limits bound the actual request and output, not hypothetical future operations. There is no ancestor depth budget or inventory-wide reservation.

## Failure and another explicit sync

An error reports its stage, OS error category and `ArtifactEffect`: `NotApplied`, `MayHaveApplied` or `Applied`. Recursive deletion can fail after partial work; catalogue persistence can fail after a successful file replacement. These cases are reported rather than described as rollback or success.

A missing owned installation can be recreated by another explicit sync. If replacement succeeded but catalogue persistence failed, and the previously registered destination now contains exactly the requested content, another sync can finish the catalogue update without rewriting those files. Other content conflicts remain errors. `forget` removes only the selected catalogue entry; it never deletes the physical destination or claims a foreign file.

Source reads, cancellation, content integrity and native store revisions remain independent of installation bookkeeping. An installation failure does not justify a global generation fence or a persistent recovery framework.
