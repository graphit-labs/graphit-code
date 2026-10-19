---
title: "Rust AST Grammar Provenance and Language Lifetimes"
description: "Source inventory, artifact boundaries and qualification requirements for all Tree-sitter grammars."
content-type: reference
audience: developers
references:
  - target: {type: task, id: tsk-9ed9}
    relation: implements
  - target: {type: task, id: tsk-50f7}
    relation: derived_from
  - target: {type: task, id: tsk-04ab}
    relation: relates_to
  - target: {type: task, id: tsk-0006}
    relation: derived_from
  - target: {type: task, id: tsk-2cf8}
    relation: relates_to
  - target: {type: task, id: tsk-e056}
    relation: relates_to
  - target: {type: task, id: tsk-0260}
    relation: relates_to
  - target: {type: task, id: tsk-090c}
    relation: relates_to
  - target: {type: task, id: tsk-fa77}
    relation: relates_to
  - target: {type: task, id: tsk-74bf}
    relation: supports
  - target: {type: task, id: tsk-861f}
    relation: relates_to
  - target: {type: task, id: tsk-de12}
    relation: relates_to
  - target: {type: task, id: tsk-d284}
    relation: relates_to
  - target: {type: task, id: tsk-1528}
    relation: relates_to
---

# Rust AST grammar provenance and language lifetimes

This reference is for AST maintainers and consumers of the Rust Tree-sitter
backend. It preserves source evidence and design constraints from `tsk-50f7`
and the completed source-provenance unit `tsk-0260`, under `tsk-04ab`.
The implemented SDK language owner and builtin static acquisition are
documented in [AST/parser contracts](rust_ast_parsers.md). Their native execution
results belong to their delivery records; the source comparisons on this page
do not qualify a compiled library or an operating system.

The source-provenance inventory has **41 entries: 25 external entries from 24 upstream grammar packages, plus 16 local grammar sources**. TypeScript and TSX share one origin but remain separate grammars. Forty entries serve shipped Tree-sitter profiles; Markdown is an opt-in block grammar without a shipped profile. This inventory supplements the full 45-profile capability map in [AST/parser contracts](rust_ast_parsers.md); the five ANTLR dialects have their own build and qualification boundaries. No language may be removed to simplify dependency selection.

## Evidence and producer boundaries

The Rust executable invokes `tree_sitter_build::build` from `crates/graphit-ast/build.rs`. The grammar SDK and parser contracts are described in [AST/parser contracts](rust_ast_parsers.md). The source identities below preserve upstream origins, customizations, licenses and generated ABI information; generated-file SHA-256 identities are not compiled artifact identities.

Graphit Libs maintains the Lance and Ladybug source forks; Tree-sitter native builds remain inputs of Graphit Code. Local models use the verified llama.cpp/cxx GGUF runtime; ONNX Runtime is not a current product prerequisite. An unavailable Hub catalog is an access limitation, not evidence that no published grammar exists. Source provenance does not by itself qualify an artifact format or a published platform build.

## Per-entry source matrix

An upstream release/tag, a generated grammar metadata version, a source-file
hash and an artifact hash have different meanings. A release pin must resolve
to an immutable source receipt. Local copyright names and metadata versions
must not be converted into guessed repository URLs or commits. A patch receipt
must identify the base bundle and the actual diff; an empty patch list requires
an exact comparison against that base.

Each completed source receipt must contain grammar ID, explicit aliases and
profile association, upstream repository and immutable commit, release when
applicable, source bundle identity, exported C symbol, generated ABI, every
scanner/support source and its language/hash, actual license/notice files and
hashes, and patch base/diff/reason. PLpgSQL additionally needs generator,
PostgreSQL grammar/keyword source and tool pins. Target build and runtime
receipts are separate records.

Origin, used-file, license and local-customization proof for the 41 rows below
was completed by **`tsk-0260`, AST source-provenance owner**. Its receipts
reuse valid comparisons, recover missing immutable bases and preserve actual
parser/scanner/header/support and license bytes. Complete upstream trees,
ZIP/H1/SumDB proofs and historical environments are not universal obligations.
Its checks compare the 41-entry coverage with source and documentation; a digest or
metadata version alone does not establish an upstream base or local patch.

### Source receipts: used files and local customizations

`tsk-0260` records the immutable bases, compared used files, explicit local
customizations and necessary generation recipes for all 41 grammars. The
auxiliary `source-receipts.json` under
`.build/ast-grammar-provenance-0260/` contains the per-file distribution digests,
actual include closures, licenses/notices and identity/profile associations.
It reuses the selected-file external evidence from `tsk-50f7` and adds the
52 previously unexamined external include headers, all byte-identical upstream.

Node PLpgSQL grammar generation and SQL/Swift parser generation were executed
as described below. Native compilation, parser/query execution, loading and
runtime/platform qualification were not executed by this provenance unit.

The Graphit introduction commit
`d87c4061a70dc446a772b525642376fa928a0ac6` explicitly identifies
`gmr/tree-sitter-postgres/plpgsql`. Comparing the five official grammar-history
revisions located the identical `grammar.js` at upstream commit
[`733cf21d2010b6127f90f219b1c8e778b0816ec3`](https://github.com/gmr/tree-sitter-postgres/tree/733cf21d2010b6127f90f219b1c8e778b0816ec3).
Its commit tree is `e27bdd1598ef169c4b1e2dfb55368e6312f1dbde`.
Seventeen fetched source and recipe files were checked against that tree's Git
blob objects with `git hash-object --stdin`. SHA-256 below hashes the original
file bytes; it is separate from Git object identity and Go module H1.

The local paths below are relative to `crates/graphit-ast/grammars/tree_sitter/plpgsql/`.
Export `tree_sitter_plpgsql`, declared ABI 15, the C scanner and C11 binding are
preserved. These declarations do not prove native ABI execution or loading.

| Local path | Upstream path | Original local SHA-256 | Comparison |
|---|---|---|---|
| `grammar.js` | `plpgsql/grammar.js` | `843d9a6cb10ab8b31e6c622634ea1b014b97a915bc92b6c809d095ec48022dfb` | Exact bytes |
| `parser.c.inc` | `plpgsql/src/parser.c` | `7f532d070ea21aaf07a11c6539d0872c0fa3a9f2ce600c0c5ccd9ebae52020ae` | Metadata patch described below |
| `scanner.c.inc` | `plpgsql/src/scanner.c` | `52b646cc5cf5c3007c88143da96fe728b2d754e851c9b093ec7e1e7196d25733` | Exact bytes |
| `node-types.json` | `plpgsql/src/node-types.json` | `070ed72af3816fb520331e188d42ed2ff6cbbd7652775f5421646bbac9559422` | Exact bytes |
| `tree_sitter/alloc.h` | `plpgsql/src/tree_sitter/alloc.h` | `b29c1c9fb7cc82f58c84b376df1297d6e2737a1d655fd356db0859e3c29c2fea` | Exact bytes |
| `tree_sitter/array.h` | `plpgsql/src/tree_sitter/array.h` | `31e60a1bff6f715afacce03b5b70efe42b58371b4f9595dd4af52a577ff9608c` | Exact bytes |
| `tree_sitter/parser.h` | `plpgsql/src/tree_sitter/parser.h` | `180b893c8734778fd32f372dfbc27bd6ad1cd2221f26150b31256ff6716320d2` | Exact bytes |
| `LICENSE` | `LICENSE` | `b336477d5469bf335e7d173814e1da57d4f13c23968959a71523668ac4a9a6c2` | Exact BSD-3-Clause text |

The upstream parser is 822,644 bytes, SHA-256
`78e6aba323735d092232785e9814531dd34a84f60a8af9cca1d31fcf33965637`;
the local parser is 822,645 bytes. The diff rewrites exactly three
`TSLanguage.metadata` fields, from major/minor/patch `1.2.4` to `19.0.0`.
The net size change of one byte does not mean a one-byte patch or a newline
change. The preserved unified diff has SHA-256
`4c5feaf90e707436965bee44bf20d38747e8dab88d416304931a89ac81f8034b`.
The historical rationale is unknown. The receipt preserves these three edits
as a local customization; it does not normalize them away or invent an intent.
No source file was replaced. The unchanged ABI constant is independent of
these grammar-version metadata fields.

The recovered `script/generate-plpgsql-grammar.js` has SHA-256
`e12122bef73977e38154c72fa0fe895763a90851eb426c78fd33ed2ae604f2a3`.
It imports `parse-plpgsql-kwlist.js`
(`5e7809db9305eda0ea2f4165ffefa6f7e58e3e19fea3d4cbcdf7dbb1e39fc485`)
and `parse-kwlist.js`
(`5299599d1c59adda3f2fe62a8e6d38bce237bd491a3dd6e54acdfa5d213d2925`).
The implementation reads `pl_reserved_kwlist.h` and `pl_unreserved_kwlist.h`,
then emits the upstream hand-structured grammar. Although its output header
cites `pl_gram.y`, it does not read or automatically translate that file.
`package.json` maps `generate:plpgsql` to this script. The checked-in lock pins
Tree-sitter CLI `0.26.9`; the package range `^0.26.7` alone is not that pin.
Neither file pins the Node executable or proves the tools originally used.

The same upstream README identifies `REL_19_BETA2`. The official PostgreSQL
ref resolves to candidate commit
[`7873db5369b967dd53984df34fd800003410d04f`](https://github.com/postgres/postgres/tree/7873db5369b967dd53984df34fd800003410d04f).
The fetched files under `src/pl/plpgsql/src/` have these original hashes:

| File | SHA-256 | Generator role |
|---|---|---|
| `pl_reserved_kwlist.h` | `c743491696d675748f80476873ec5297b43cb2f7886ab8a54764a3f04d311b34` | Read input |
| `pl_unreserved_kwlist.h` | `89dfe9c35247a39384cd5cc0f1cfd4b196e63e0d4b344b42a247218420008ac2` | Read input |
| `pl_gram.y` | `8463fc9fb68a17c5198e604bc21cc4f4b41926afa71254337d692e6b42cb6fdc` | Conceptual source, not read by this generator |

The keyword-to-output comparison now establishes this PostgreSQL base for
`grammar.js`. Running `node script/generate-plpgsql-grammar.js postgres` from
an auxiliary copy of the pinned recipe under Node `v22.14.0` read 22 reserved
and 85 unreserved keywords. It produced 32,703 bytes exactly equal to the local
`grammar.js`, including SHA-256
`843d9a6cb10ab8b31e6c622634ea1b014b97a915bc92b6c809d095ec48022dfb`.
Only the two keyword headers were supplied under the explicit `postgres` input
path. Node `v22.14.0` identifies this execution, not the historical environment.

The second stage is `tree-sitter generate` inside `plpgsql`, using the CLI
`0.26.9` declared by the upstream lock. The upstream `justfile` target
`generate-plpgsql` runs only this second stage; it does not run the keyword
script. Parser regeneration was unnecessary for the already isolated three-field
metadata customization and was not executed. Existing upstream-generated files
remain the base; the local patch remains explicit.

The current auxiliary receipt is
`.build/ast-grammar-provenance-0260/plpgsql-generation/generation-result.json`;
`inputs.json` records the recovered recipe and keyword inputs. Earlier source
comparisons were stored in `.build/rust-migration/ast-grammar-sourceproof/`,
which is absent from this checkout. Their verified identities and comparisons
remain in this document and the Task records; the current work reuses that
specific evidence without claiming that the missing auxiliary files exist.
Neither complete ZIP/H1/SumDB proofs nor an exact historical tool environment
are universal provenance requirements. Registry metadata and source comparisons
confer no native loading, runtime qualification or publication authority.

#### Vue source comparison

The Vue introduction commit
`cbf96e1f561339c3b767cb1432f2e1838503188c` records upstream
`tree-sitter-grammars/tree-sitter-vue` at short commit `ce8011a4`.
The official commit lookup resolves this to
[`ce8011a414fdf8091f4e4071752efc376f4afb08`](https://github.com/tree-sitter-grammars/tree-sitter-vue/tree/ce8011a414fdf8091f4e4071752efc376f4afb08),
tree `d9610e1230c672902b5b36cfbde51f3cca75f1a9`.
Its complete API tree has 64 entries. Ten retrieved source/recipe blobs match
their pinned Git objects. All seven compared local native source, support and
license files are byte-identical; their local paths below are relative to
`crates/graphit-ast/grammars/tree_sitter/vue/`.

| Local path | Upstream path | SHA-256 of both original files |
|---|---|---|
| `parser.c.inc` | `src/parser.c` | `770f6ec24e908a30adf26056cc20375d94e2918bc79c0bdff4331d1dbd5577ce` |
| `scanner.c.inc` | `src/scanner.c` | `9c2147fe2de1ede71f3ef5fd3ba54d12dbb6c2f71bce5735358fdbd4f8d19a32` |
| `tag.h` | `src/tag.h` | `b639821160ac0e1a70d1dc2fe206380da945bd65be7a010ffdb90694a0fef2b8` |
| `tree_sitter/alloc.h` | `src/tree_sitter/alloc.h` | `b29c1c9fb7cc82f58c84b376df1297d6e2737a1d655fd356db0859e3c29c2fea` |
| `tree_sitter/array.h` | `src/tree_sitter/array.h` | `5bdf6ed1a78e3409fd443e085ca967a64c188a5d082aaf7f819bccd53a471c94` |
| `tree_sitter/parser.h` | `src/tree_sitter/parser.h` | `180b893c8734778fd32f372dfbc27bd6ad1cd2221f26150b31256ff6716320d2` |
| `LICENSE` | `LICENSE` | `91ebb180e4e2e34628c45e405584d0faac6863660b69e8139da96290e58c14dd` |

The empty native-source diff is established for these seven files only.
These seven files establish the used native sources, support and license at
this immutable base without a local patch. They do not prove every upstream
file or runtime qualification. The historical auxiliary
`vue-source-comparison-01.json` is absent from this checkout; its ten fetched
blob identities and seven-file comparison remain recorded in `tsk-0260`.
Identity/profile association is checked together with the 41-entry inventory.
Existing bindings and source files were preserved without recompilation.

#### Additional local comparisons

These immutable bases were found through official upstream objects and compared
with the files distributed under `crates/graphit-ast/grammars/tree_sitter/<id>/`. Matching
metadata or a copyright string was only a locator; it was not accepted as
origin proof. The auxiliary receipts are under
`.build/ast-grammar-provenance-0260/local-candidates/`. Every listed exact-file
count includes the parser, existing scanner, distributed Tree-sitter headers,
extra support and license file; retired language-binding wrappers are not native parser or scanner inputs.

| Grammar | Official repository / immutable base | Used-file result |
|---|---|---|
| dart | [`nielsenko/tree-sitter-dart@b57d734c84f510bbd524097902cab671e4dbfca9`](https://github.com/nielsenko/tree-sitter-dart/tree/b57d734c84f510bbd524097902cab671e4dbfca9) | 6/6 exact files |
| dockerfile | [`camdencheek/tree-sitter-dockerfile@439c3e7b8a9bfdbf1f7d7c2beaae4173dc484cbf`](https://github.com/camdencheek/tree-sitter-dockerfile/tree/439c3e7b8a9bfdbf1f7d7c2beaae4173dc484cbf) | 6/6 exact files |
| elixir | [`elixir-lang/tree-sitter-elixir@4b0c7118760af58a2e7081bbc8396e136f820b37`](https://github.com/elixir-lang/tree-sitter-elixir/tree/4b0c7118760af58a2e7081bbc8396e136f820b37) | 7/7 exact files, including official NOTICE added |
| graphql | [`bkegley/tree-sitter-graphql@5e66e961eee421786bdda8495ed1db045e06b5fe`](https://github.com/bkegley/tree-sitter-graphql/tree/5e66e961eee421786bdda8495ed1db045e06b5fe) | 3/3 exact files |
| groovy | [`murtaza64/tree-sitter-groovy@2a6ddd558b6aa39c5b77d8db9fe9baf817486b2c`](https://github.com/murtaza64/tree-sitter-groovy/tree/2a6ddd558b6aa39c5b77d8db9fe9baf817486b2c) | 5/5 exact files |
| kotlin | [`fwcd/tree-sitter-kotlin@0ef87892401bb01c84b40916e1f150197bc134b1`](https://github.com/fwcd/tree-sitter-kotlin/tree/0ef87892401bb01c84b40916e1f150197bc134b1) | Parser, parser.h and LICENSE exact; scanner customization below |
| markdown | [`tree-sitter-grammars/tree-sitter-markdown@a0a00f817d02412bd92c54d316f164d827b57b5c`](https://github.com/tree-sitter-grammars/tree-sitter-markdown/tree/a0a00f817d02412bd92c54d316f164d827b57b5c) | 6/6 exact files |
| proto | [`coder3101/tree-sitter-proto@f0e767689c6e9efe7ff2e4b8680db71411915588`](https://github.com/coder3101/tree-sitter-proto/tree/f0e767689c6e9efe7ff2e4b8680db71411915588) | 5/5 exact files |
| r | [`r-lib/tree-sitter-r@daa26a2ff0d9546e9125c7d8bcec046027f02070`](https://github.com/r-lib/tree-sitter-r/tree/daa26a2ff0d9546e9125c7d8bcec046027f02070) | 6/6 exact files |
| svelte | [`tree-sitter-grammars/tree-sitter-svelte@ae5199db47757f785e43a14b332118a5474de1a2`](https://github.com/tree-sitter-grammars/tree-sitter-svelte/tree/ae5199db47757f785e43a14b332118a5474de1a2) | 7/7 exact files |

For Markdown, the upstream prefix is `tree-sitter-markdown/src/`, selecting the
block grammar; no inline grammar is substituted. Other table entries use
`src/`. The local `.inc` filenames preserve the corresponding upstream `.c`
bytes. Each scanner is C, and each exported symbol remains
`tree_sitter_<grammar_id>` for these entries.

Kotlin's base is `0ef87892401bb01c84b40916e1f150197bc134b1`. Its local scanner
replaces exactly four `exit(1)` calls with `abort()` in `new_stack`, `push` and
`pop`. This is a real local customization. The parser, header and MIT license
are unchanged; no normalization hides the scanner diff. Historical intent is
not established and is not invented by the receipt.

Clojure's parser and `tree_sitter/parser.h` are exact at
[`sogaiu/tree-sitter-clojure@e43eff80d17cf34852dcd92ca5e6986d23a7040f`](https://github.com/sogaiu/tree-sitter-clojure/tree/e43eff80d17cf34852dcd92ca5e6986d23a7040f).
The upstream text is `COPYING.txt`, CC0 1.0, SHA-256
`a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499`.
It is now distributed verbatim as local `COPYING.txt`. The existing local
`LICENSE` contains MIT text attributed to sogaiu; its source is unproven and
it is preserved as a local divergence. It does not establish upstream MIT
licensing or a relicensing decision. The upstream `COPYING.txt` has been present
since commit `f09652f095be878df8a87a57dcbfa07094316253`.

Objective-C's base is
[`tree-sitter-grammars/tree-sitter-objc@7d5b1733b9006b7d4388f097745c8e10e0c30a2a`](https://github.com/tree-sitter-grammars/tree-sitter-objc/tree/7d5b1733b9006b7d4388f097745c8e10e0c30a2a).
Its header is exact. The parser differs only in the first directive,
`#include <tree_sitter/parser.h>` becoming `#include "tree_sitter/parser.h"`.
The local MIT text changes the title and copyright attribution from the
upstream Amaan Qureshi text. These license edits remain explicit; the local
attribution is not upstream proof. The official upstream MIT text is now distributed unchanged as
`crates/graphit-ast/grammars/tree_sitter/objc/LICENSE.upstream`, while the existing `LICENSE`
is preserved with its attribution edits recorded as a local customization.

SQL's grammar input at
[`DerekStride/tree-sitter-sql@1156b55f26a5949784827f21c6abc72c05e26600`](https://github.com/DerekStride/tree-sitter-sql/tree/1156b55f26a5949784827f21c6abc72c05e26600)
was verified against its official Git blob. Running
`tree-sitter generate --no-bindings` with CLI `0.22.6` and Node `v22.14.0` in an
auxiliary directory reproduced `parser.c.inc` and all three local Tree-sitter
headers byte for byte. The scanner and MIT license are exact upstream sources.
The parser SHA-256 is
`2b389464426691c26bb289cac391aaf48aeec868dd1c039a2efb44f3ac394ad5`.
Generation took 44.20 seconds and used 3,355,840 KiB maximum RSS on Linux;
no native compilation or parser execution occurred. The reproduction receipt
is `.build/ast-grammar-provenance-0260/sql-generation/generation-result.json`.
The alternate crates.io parser differed and was not accepted as the local base.

Swift's input base is
[`alex-pinkus/tree-sitter-swift@28fe3a8a85586aa297524fe6164140b9521dcaff`](https://github.com/alex-pinkus/tree-sitter-swift/tree/28fe3a8a85586aa297524fe6164140b9521dcaff).
The input `grammar.js`, `package.json` and `tree-sitter.json` match their pinned
Git blobs. CLI `0.22.6`, Node `v22.14.0` and the same command
`tree-sitter generate --no-bindings` reproduced the local parser (20,655,828
bytes) and three Tree-sitter headers byte for byte. Scanner and MIT license
are exact at the pin. The reproduction receipt is
`.build/ast-grammar-provenance-0260/swift-generation/generation-result.json`.
The different crate `0.7.1` was a rejected candidate, not a local patch base.
No native compilation or parser execution occurred.

The receipt inventory contains 41 unique grammar IDs and 40 profiles plus
opt-in Markdown. Declared ABIs 13/14/15 occur 1/21/19 times; 31 entries have C
scanners and 10 have none. Compiler preprocessing with `cc -MM` confirmed the
actual native include closures without compiling or executing a parser. The
remaining supplied local headers are preserved as distributed support files.

The associations preserve profile `csharp`, grammar ID `c-sharp` and explicit alias `c_sharp`, and profile `protobuf` with grammar ID `proto`. TypeScript and TSX keep separate parsers/exports despite sharing one origin. PLpgSQL omits a `grammar` YAML field; its effective grammar is `tree-sitter-plpgsql`. Thus 39 explicit YAML declarations plus this PLpgSQL default give 40 profiles; its exclusive profile is not selected automatically by extension. No generic underscore normalization is introduced.

The 41-entry source inventory is reconciled with the source associations,
profile files, native exports/ABI/scanners, effective includes, distribution
bytes and original license/notice texts. The auxiliary validation command is
`python3 .build/ast-grammar-provenance-0260/validate-source-receipts.py`.
Clojure's preserved MIT attribution remains of unproven origin, and historical
intent behind the explicit local edits remains unknown where stated. These
unknowns are not presented as upstream licensing or omitted from the receipt.
None of these source results qualifies a runtime or platform.

Elixir's upstream `NOTICE` is now distributed verbatim as
`crates/graphit-ast/grammars/tree_sitter/elixir/NOTICE`, SHA-256
`119588423fbaa46a6955a044fee5a23338c7de190d8c4651a94c6d0b98ca2cda`.
The Apache `LICENSE` remains unchanged. The notice preserves MIT terms for
the generated source portion, Apache terms for the remaining source portion,
and its other original attributions. Its historical `scanner.cc` spelling is
preserved even though the current scanner is C; the receipt does not rewrite
or simplify the upstream legal text.

### External module entries

The versions below are observed `go.mod` pins, not approved Rust crate pins or
native runtime receipts. The source inspection receipt below establishes the
selected commit/export/ABI/scanner/license comparisons. Source closure retains
the files actually distributed and any local customization; it does not require
a complete upstream-tree comparison. None of the 25 rows is runtime-qualified.

| Grammar ID | Upstream module / release pin | Recorded source scope |
|---|---|---|
| bash | `github.com/tree-sitter/tree-sitter-bash v0.25.1` | Used-file receipt; included headers verified |
| c | `github.com/tree-sitter/tree-sitter-c v0.24.2` | Same used-file receipt |
| cpp | `github.com/tree-sitter/tree-sitter-cpp v0.23.4` | Same used-file receipt |
| c-sharp | `github.com/tree-sitter/tree-sitter-c-sharp v0.23.5` | Same used-file receipt |
| css | `github.com/tree-sitter/tree-sitter-css v0.25.0` | Same used-file receipt |
| go | `github.com/tree-sitter/tree-sitter-go v0.25.0` | Same used-file receipt |
| haskell | `github.com/tree-sitter/tree-sitter-haskell v0.23.1` | Same used-file receipt |
| html | `github.com/tree-sitter/tree-sitter-html v0.23.2` | Same used-file receipt |
| java | `github.com/tree-sitter/tree-sitter-java v0.23.5` | Same used-file receipt |
| javascript | `github.com/tree-sitter/tree-sitter-javascript v0.25.0` | Same used-file receipt |
| json | `github.com/tree-sitter/tree-sitter-json v0.24.8` | Same used-file receipt |
| julia | `github.com/tree-sitter/tree-sitter-julia v0.25.0` | Same used-file receipt |
| php | `github.com/tree-sitter/tree-sitter-php v0.24.2` | PHP grammar, not PHP-only grammar; used-file receipt |
| python | `github.com/tree-sitter/tree-sitter-python v0.25.0` | Same used-file receipt |
| ruby | `github.com/tree-sitter/tree-sitter-ruby v0.23.1` | Same used-file receipt |
| rust | `github.com/tree-sitter/tree-sitter-rust v0.24.2` | Same used-file receipt |
| scala | `github.com/tree-sitter/tree-sitter-scala v0.26.0` | Same used-file receipt |
| typescript | `github.com/tree-sitter/tree-sitter-typescript v0.23.2` | `typescript` generated source and export; used-file receipt |
| tsx | `github.com/tree-sitter/tree-sitter-typescript v0.23.2` | Separate `tsx` generated source and export; used-file receipt |
| hcl | `github.com/tree-sitter-grammars/tree-sitter-hcl v1.2.0` | Same used-file receipt |
| lua | `github.com/tree-sitter-grammars/tree-sitter-lua v0.5.0` | Same used-file receipt |
| toml | `github.com/tree-sitter-grammars/tree-sitter-toml v0.7.0` | Same used-file receipt |
| xml | `github.com/tree-sitter-grammars/tree-sitter-xml v0.7.0` | XML grammar, not DTD grammar; used-file receipt |
| yaml | `github.com/tree-sitter-grammars/tree-sitter-yaml v0.7.2` | Same used-file receipt |
| zig | `github.com/tree-sitter-grammars/tree-sitter-zig v1.1.2` | Same used-file receipt |

### External source inspection receipt

The 24 pinned module tags were resolved through the official GitHub Git-ref API,
including peeling annotated tags; every commit matched the cached module's
`Origin.Hash`. The selected parser, scanner, binding, license and additional
support files matched the cached ZIP byte for byte. Recorded `.ziphash` values
matched the corresponding `go.sum` entries. The canonical complete ZIP hash and
SumDB verification chain were not recomputed, and the complete ZIP was not
compared against the upstream Git tree. No local change was observed in the
selected files; this does not establish a historical full-tree patch proof.

The resulting source ABI distribution is 13 ABI15 and 12 ABI14. Twenty external
entries have C scanners and five have none. Each license is `LICENSE`, except
Lua's `LICENSE.md`. Preserve exact license text; HCL is Apache-2.0, the other
external module licenses inspected are MIT.

| Grammar | Official commit | Generated ABI / export | Scanner and specific support source | License |
|---|---|---|---|---|
| bash | `a06c2e4415e9bc0346c6b86d401879ffb44058f7` | 15 / `tree_sitter_bash` | src/scanner.c | MIT |
| c | `b780e47fc780ddc8da13afa35a3f4ed5c157823d` | 15 / `tree_sitter_c` | none | MIT |
| c-sharp | `cac6d5fb595f5811a076336682d5d595ac1c9e85` | 15 / `tree_sitter_c_sharp` | src/scanner.c | MIT |
| cpp | `f41e1a044c8a84ea9fa8577fdd2eab92ec96de02` | 14 / `tree_sitter_cpp` | src/scanner.c | MIT |
| css | `dda5cfc5722c429eaba1c910ca32c2c0c5bb1a3f` | 15 / `tree_sitter_css` | src/scanner.c | MIT |
| go | `1547678a9da59885853f5f5cc8a99cc203fa2e2c` | 15 / `tree_sitter_go` | none | MIT |
| haskell | `c30d812bc90827f1a54106a25bc9a6307f5cdcec` | 14 / `tree_sitter_haskell` | src/scanner.c + unicode.h | MIT |
| html | `5a5ca8551a179998360b4a4ca2c0f366a35acc03` | 14 / `tree_sitter_html` | src/scanner.c + tag.h | MIT |
| java | `94703d5a6bed02b98e438d7cad1136c01a60ba2c` | 14 / `tree_sitter_java` | none | MIT |
| javascript | `44c892e0be055ac465d5eeddae6d3e194424e7de` | 15 / `tree_sitter_javascript` | src/scanner.c | MIT |
| json | `ee35a6ebefcef0c5c416c0d1ccec7370cfca5a24` | 14 / `tree_sitter_json` | none | MIT |
| julia | `e0f9dcd180fdcfcfa8d79a3531e11d99e79321d3` | 15 / `tree_sitter_julia` | src/scanner.c | MIT |
| php | `5b5627faaa290d89eb3d01b9bf47c3bb9e797dea` | 15 / `tree_sitter_php` | php/src/scanner.c + common/scanner.h | MIT |
| python | `293fdc02038ee2bf0e2e206711b69c90ac0d413f` | 15 / `tree_sitter_python` | src/scanner.c | MIT |
| ruby | `71bd32fb7607035768799732addba884a37a6210` | 14 / `tree_sitter_ruby` | src/scanner.c | MIT |
| rust | `77a3747266f4d621d0757825e6b11edcbf991ca5` | 15 / `tree_sitter_rust` | src/scanner.c | MIT |
| scala | `38950b525c9dfc44c8b60d44bdd6e54217286ca8` | 15 / `tree_sitter_scala` | src/scanner.c | MIT |
| typescript | `f975a621f4e7f532fe322e13c4f79495e0a7b2e7` | 14 / `tree_sitter_typescript` | typescript/src/scanner.c + common/scanner.h | MIT |
| tsx | `f975a621f4e7f532fe322e13c4f79495e0a7b2e7` | 14 / `tree_sitter_tsx` | tsx/src/scanner.c + common/scanner.h | MIT |
| hcl | `fad991865fee927dd1de5e172fb3f08ac674d914` | 15 / `tree_sitter_hcl` | src/scanner.c | Apache-2.0 |
| lua | `10fe0054734eec83049514ea2e718b2a56acd0c9` | 15 / `tree_sitter_lua` | src/scanner.c | MIT |
| toml | `64b56832c2cffe41758f28e05c756a3a98d16f41` | 14 / `tree_sitter_toml` | src/scanner.c | MIT |
| xml | `4b64dd3a03ec002258d6268d712fd93716d6ab57` | 14 / `tree_sitter_xml` | xml/src/scanner.c + common/scanner.h | MIT |
| yaml | `7708026449bed86239b1cd5bce6e3c34dbca6415` | 14 / `tree_sitter_yaml` | src/scanner.c + schema.core.c | MIT |
| zig | `b670c8df85a1568f498aa5c8cae42f51a90473c0` | 14 / `tree_sitter_zig` | none | MIT |

All non-empty scanner rows are C; retain each generated Tree-sitter header and
the actual include closure when producing the bundle. YAML's scanner defines
`YAML_SCHEMA core` by default and the native scanner preserves that schema. Preserve
`src/schema.core.c` (SHA-256
`91e56c3f3ae6fad1803b739ce4eb4568a782a9aec710f733db2f3e38d407a4d5`).
Do not substitute PHP-only, DTD, Terraform or a different YAML schema. TypeScript
and TSX share their origin commit but have separate generated parsers and exports.

### Local generated bindings

All paths in this table are under `crates/graphit-ast/grammars/tree_sitter/<id>/`. Every binding
includes `parser.c.inc` with C11 compilation flags. Eleven bindings also include
`scanner.c.inc` as C; five have no scanner translation unit. The ABI comes from
the generated `LANGUAGE_VERSION` constant associated with `TSLanguage`, not
from a runtime library execution. The local distribution is one ABI 13, nine
ABI 14 and six ABI 15.

| ID / exported symbol | ABI | Scanner | Observed local license files; base and customizations in Source receipts |
|---|---:|---|---|
| clojure / `tree_sitter_clojure` | 14 | None | CC0 `COPYING.txt`; preserved local MIT `LICENSE` origin unproven |
| dart / `tree_sitter_dart` | 15 | C | MIT `LICENSE`; binding cites nielsenko; metadata 0.2.0 |
| dockerfile / `tree_sitter_dockerfile` | 14 | C | MIT `LICENSE`; Camden Cheek copyright |
| elixir / `tree_sitter_elixir` | 14 | C | Apache-2.0 `LICENSE` plus official `NOTICE` with generated-source MIT terms |
| graphql / `tree_sitter_graphql` | 13 | None | MIT `LICENSE`; bkegley copyright |
| groovy / `tree_sitter_groovy` | 15 | None | MIT `LICENSE`; generator header 0.25.10; grammar metadata 0.0.1 |
| kotlin / `tree_sitter_kotlin` | 14 | C | MIT `LICENSE`; fwcd copyright; scanner cites JavaScript source without pin |
| markdown / `tree_sitter_markdown` | 15 | C | MIT `LICENSE`; block grammar; metadata 0.5.3 |
| objc / `tree_sitter_objc` | 14 | None | Official MIT `LICENSE.upstream`; local `LICENSE` attribution edits preserved |
| plpgsql / `tree_sitter_plpgsql` | 15 | C | BSD-3-Clause `LICENSE`; generated-source coupling; metadata 19.0.0 |
| proto / `tree_sitter_proto` | 15 | None | MIT license text in `LICENSE`, preserving its specific wording; metadata 0.4.0 |
| r / `tree_sitter_r` | 14 | C | MIT `LICENSE`; scanner's R-source commit is not a grammar commit |
| sql / `tree_sitter_sql` | 14 | C | MIT `LICENSE`; Derek Stride copyright |
| svelte / `tree_sitter_svelte` | 14 | C | MIT `LICENSE`; scanner also needs local `tag.h` |
| swift / `tree_sitter_swift` | 14 | C | MIT `LICENSE`; alex-pinkus copyright |
| vue / `tree_sitter_vue` | 15 | C | MIT license text in `LICENSE`; local `tag.h`; metadata 0.1.0 |

Only PLpgSQL has local `grammar.js` and `node-types.json`. Its grammar header
describes `script/generate-plpgsql-grammar.js`, PostgreSQL
`src/pl/plpgsql/src/pl_gram.y` and keyword lists, with opaque SQL handled through
injection. Those generator/source/tool pins are not supplied by the local
directory. Preserving its parser means recovering that provenance, not replacing
it with an improvised Rust grammar or treating metadata 19.0.0 as a source pin.

All 16 parsers use `tree_sitter/parser.h`. Clojure, GraphQL, Kotlin and Objective-C
have only that local Tree-sitter header; the other twelve also supply
`tree_sitter/array.h` and `alloc.h`. Svelte/Vue `tag.h` transitively use those
support headers. Their hashes and ancestry belong in the eventual bundle
receipt, alongside parser and scanner identities.

| ID | `parser.c.inc` SHA-256 | `scanner.c.inc` SHA-256, if present |
|---|---|---|
| clojure | `a13eafdaf733f597c4cd2ba038834a37b06c4616ec907869504657829d8e3378` | — |
| dart | `1894ef57f8e0ceb483ccf6d548429517dc42b0d983293ebdb5ff9ff9c612c4a9` | `7d9fc9245dc2532bb666f737d4b8c48dc2c465ace4677a17d11496ca1268cbd5` |
| dockerfile | `5ed4ed9ede55fbf4c3764419fdad6cbe6422547f398c1a01377dfbfacd3fadd2` | `1080c2eb2ac41f102974e009cb62f644d9d638dd2468d0a700674d07d346fde7` |
| elixir | `d1c2000b477e873e44e485f55809e3af7e2bfce83fa4d58cfa3a9f83660e7d79` | `2d49d8974f7fdf0d38bdb7ed792f7a60aa87dd087b0c0bb469cd5b138b08ba57` |
| graphql | `b2ee5d6f514ea72dddd8f34516ef57ec50329d7171c60c03b088f5f5f39cc390` | — |
| groovy | `5ec80af680626dbe87cac861678f8bd102bdf6be474eeb8e3d321ef73df9b33e` | — |
| kotlin | `9e3f36d0b22813c39cc54e49d5912107424a57aa1ec375c3720c8572d44a5c8b` | `e5437ae6d5ea70106c83fbf0251fbdb59f4829abc8f30881daa68c56bd97f12b` |
| markdown | `0f51c93b5d79d08bc74088d91a9aafb7e40ad7e2f169164865c01d392277bb07` | `02834bcbaf0cf51178e74450d487cc0231b9a52541cd374527ee35cfaf17fd4a` |
| objc | `954a6c7be10bd8db2d71cc303cc9aed7128cbe8524e3a2e99c7d1461e30d451c` | — |
| plpgsql | `7f532d070ea21aaf07a11c6539d0872c0fa3a9f2ce600c0c5ccd9ebae52020ae` | `52b646cc5cf5c3007c88143da96fe728b2d754e851c9b093ec7e1e7196d25733` |
| proto | `3ab67f91aef2892b664103b87e643f744b1a87e42f857df932a920344e5cf468` | — |
| r | `622165734714c6e81f70d99d522d78cf98915fdf2b1032f8bb33e775a0b971c0` | `1209d1107616b470a4292c18d57b56b44990c39792acf7af6ccada0af563dbff` |
| sql | `2b389464426691c26bb289cac391aaf48aeec868dd1c039a2efb44f3ac394ad5` | `de17b5cffc3c86f56cf6630abb969330c3c839a81ece0b901bdac0ecee93c403` |
| svelte | `76778ca4d425582cad25f725c6dbea6b87c1710a8f4089c24eadfbd516d2ef03` | `878988200a5e2c77cc822cc0a2c69f4fc593ba76a734a6a36b9370d7501b09ce` |
| swift | `d3c13ab8e79eae586b662533d5b3ce4692a92ead5a9afe6b5310f794764b86af` | `f3d6271d64f58c39eed544104a70ca2cf9ecbf80c5d900620f1afd38836542cb` |
| vue | `770f6ec24e908a30adf26056cc20375d94e2918bc79c0bdff4331d1dbd5577ce` | `9c2147fe2de1ede71f3ef5fd3ba54d12dbb6c2f71bce5735358fdbd4f8d19a32` |

Additional observed local hashes:

- Svelte `tag.h`: `38d0f2356661128c2598660271ee9cd061ab50e7ca42b8275118f005899da47f`.
- Vue `tag.h`: `b639821160ac0e1a70d1dc2fe206380da945bd65be7a010ffdb90694a0fef2b8`.
- PLpgSQL `grammar.js`: `843d9a6cb10ab8b31e6c622634ea1b014b97a915bc92b6c809d095ec48022dfb`.
- PLpgSQL `node-types.json`: `070ed72af3816fb520331e188d42ed2ff6cbbd7652775f5421646bbac9559422`.

## SDK selection evidence

The canonical product minimum and pinned toolchain are **Rust 1.98**:
`Cargo.toml:7` and completed producer `tsk-74bf` establish that baseline. Reuse
that producer; no downgrade or additional MSRV-change delivery is required.
The engine uses exact `tree-sitter =0.27.0`. Its minimum compiler requirement
(1.90) and the Lance producer's requirement (1.91) are distinct from the
product's 1.98 minimum.
The coordinator owns shared manifest/lock activation; this document activates
nothing. The earlier 1.89 baseline inference and intermediate 1.91 proposal
were corrected against authoritative source before metadata implementation.

[`tree-sitter =0.27.0`](https://raw.githubusercontent.com/tree-sitter/tree-sitter/6070dbfefd326bd735e5683eb128cc1b57dad0c0/lib/Cargo.toml)
is MIT, MSRV 1.90, edition 2024, commit
`6070dbfefd326bd735e5683eb128cc1b57dad0c0`, published crate checksum
`2038684e0058edba0d17302619f62eabce4a8e11c6ac59506996a8d79848851d`.
The checksum is an official-index declaration; no downloaded crate archive was
compared or executed in this analysis.

`Language::abi_version`, `Language::is_parseable`, `Parser::set_language` and
`Query::new` are available. The runtime accepts generated ABI 13 through 15.
`is_parseable` tests the language pointer and lexer availability; it does not
establish provenance, native safety or grammar semantics. `set_language` rejects
unsupported ABI and a non-parseable language. Do not inspect opaque `TSLanguage`
layout to replace the mature API. Use `Query::new`, which includes Rust
predicate validation; `new_raw` is not an equivalent substitute.

The alternative 0.26.13 was investigated and not selected. It supports ABI
13–15 with a lower MSRV, but lacks `is_parseable` and returns static annotations
for language-owned strings. Maintaining the old compiler baseline does not
justify a compatibility layer or attributing newer APIs to that release. The
current product baseline already supports the selected SDK's minimum.

## Native acquisition and provenance evidence

The source inventory above records explicit source coverage, digests, licenses,
patches and generation evidence from `tsk-0260`. Source proof remains distinct
from native runtime qualification. The distributed grammar files preserve the
corresponding license and notice texts.

`tree_sitter_engine::builtin(id)` acquires a literal key from the closed
41-grammar catalogue. The executable links 39 vendored C bundles plus the
existing Rust and JSON SDK crates; intermediate archives belong to Cargo
`OUT_DIR`. Static grammar code lives for the executable lifetime.

Custom parser acquisition uses Docker images configured by extension through
`ast.scip.parsers`; see [parser resources](rust_ast_parser_resources.md).
Profile YAML supplies a literal grammar identifier for linked fallback parsing.
Cooperative Original cancellation/deadline checks preserve actual errors; they
do not establish hard RSS, CPU or preemption bounds.

## Implemented language ownership

[`GrammarLanguage`](../../crates/graphit-ast/src/tree_sitter_engine/owner.rs)
retains an Arc-owned SDK Language supplied by a statically linked grammar SDK.
`Parser::set_language` validates compatibility. Native trees and compiled queries
retain the language owner; nodes borrow their tree. An old borrower therefore
retains its actual generation. Reloading profile/query configuration preserves
existing borrowers and linked grammar code has executable lifetime.

## Delivered producers and evidence boundaries

| Producer | Delivered outcome | Evidence boundary |
|---|---|---|
| `tsk-0260` | Immutable origins, used files, licenses, patches and generators for all 41 grammars | Source proof; no native runtime claim |
| `tsk-8aee` / `tsk-04ab` | SDK engine, retained owners and linked acquisition of all 41 grammars | Linux native parse/query and tested controls |
| `tsk-1479` / `tsk-589d` / `tsk-113b` / `tsk-fb73` / `tsk-b197` | Five official customized ANTLR families, runtime and bridge | Their own C++ build/parser fixtures; not TS proof |
| `tsk-fa77` | Contracts/readiness for the extension delivery | Planning; functional results belong to A/B/C |
| `tsk-de12` | Generic native semantics and Main Graph/Search | [Generic model](rust_tree_sitter_generic_model.md), real owners/ranges/diagnostics |
| `tsk-d284` | NativeSource and NativeNN3 with Main inference/publication/query/reuse | [NN contract and qualification](rust_tree_sitter_native_nn.md); 21-field schema |

Each result keeps its original platform and scope. The source matrices above
remain useful provenance; they are not pending implementation gates or proof of
all-language semantics. Invalid queries, ABI, Original bindings and
nonexact mappings remain errors or explicit diagnostics at their actual layer.
No generic spelling normalization, parser fallback or fabricated range is
introduced by this reconciliation.
