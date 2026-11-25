# Worked feature: authorized archive browsing

This reference describes coordinator-owned workflows. For delegated roles, apply the current protocol:
{% include "protocol.md" %}

The coordinator owns acceptance, checks, checkpoints and lifecycle. This fictional case demonstrates a complete specification, graph and evidence map. Its domain labels and hypothetical results are not saved IDs, live grants, executed checks or native call arguments. Use [planning.md](planning.md) and the actual MCP schema to save the packets.

## Request, evidence and contract

Readers need an opt-in archive view; existing active-only behavior and membership permissions remain. Inspection established that the current handler, repository query and count share a visibility predicate, and that existing paging/error helpers are suitable. The UI already has query-driven paging. The user authorizes implementation after the plan, with no archive mutation, report export or permission redesign.

J1: open the default list; J2: enable archives and page through only visible rows; J3: recover from invalid input or a repository error while preserving the selected mode.

| Requirement | Observable contract |
|---|---|
| R1 | Omitted or false include_archived preserves current active-only IDs, count and ordering. |
| R2 | True includes visible active/archived rows; inaccessible rows never affect items or total. |
| R3 | Malformed or repeated selector values return the established invalid-query response; no silent coercion. |
| R4 | Ordering, page and page-size constraints use existing helpers and match the count predicate. |
| R5 | UI toggle is accessible, resets to page one and retains selection across paging. |
| R6 | Error/retry preserves the selected mode and existing error representation; no partial success. |
| R7 | API/user docs explain default, selector, paging, errors and retry and match executed behavior. |

Inputs use the existing request parser and a strict optional Boolean rule. This is a consumer of current membership authorization, not a new grant. The repository uses one parameter object/predicate for rows and count; do not copy the whole dataset into memory to filter it. Exact helper signatures and file ownership must be verified in the actual project before saving work.

## Fixture and cases

Use a temporary store with four projects: p01 active/visible, p02 archived/visible, p03 active/inaccessible, p04 archived/inaccessible. Under the same real test principal, default/false return p01,total1; true returns p01+p02,total2. Page-size one and page two under true return p02. The inaccessible records never appear in either count.

Malformed/repeated selectors return the existing error envelope. Inject a repository failure through its established test seam, not mock authorization; expect that finite error with no partial items/count. In the UI test, toggle, page, receive an error, retry and return to default while preserving the documented transitions. Existing component/handler suites and a focused combined journey prove the boundary, not a whole deployment.

## Saved packets

P, E, A, U and V below are prose aliases; bind actual responses before using IDs. Each packet supplies native `checks` containing at least one acceptance and one test kind. Save them with ordinary advertised creates, not a made-up batch.

| Unit | Description and edit ownership | Acceptance / method |
|---|---|---|
| P planning | Save the contract above, exact current source/doc locators, decisions and graph; no implementation. | P1 complete unambiguous contracts; PT1 read back all packets/IDs/links and coverage without gaps/cycles. |
| E delivery | Own R1–R7 integrated archive behavior and its final product/doc evidence. | E1 actual authorized default/archive/paging/error/retry journey; ET1 run the combined fixture and compare all relevant guides. |
| A API | Own the handler/options/repository predicate and API guide sections. Depend only on P's settled contract. Use real parsing, authorization and query helpers. | A1 R1–R4; AT1 default/false/true/paging with exact IDs/counts; AT2 malformed/repeated/error without leaks or partial success; AT3 source ↔ API documentation. |
| U UI | Own toggle, URL/state, component regression and user guide. Depend on P and actual A API. Reuse existing components/tokens/keyboard/focus/error patterns. | U1 R5–R6; UT1 toggle/page/error/retry with preserved mode; UT2 user guide matches tested flow. |
| V integration | Consume actual A/U, prove the complete visible journey and docs. Depend on A/U. | V1 R1–R7 together; VT1 combined fixture plus API/user guide examples and links. |

Create P and E before A/U/V; create A before the U dependency is saved, then V after both producer IDs exist. E owns A/U/V by parent_id. A child never depends on E. Dependencies are actual needed results, not permission to touch producer files. Confirm ownership again before edits and return producer defects to their owner.

A's description must retain its scope and known helpers, shared query/count contract, invalid/denied/error cases, exact tests and guide sections. U's description must retain keyboard/focus, mode/paging/retry behavior, API dependency, test action/results and guide. A short label such as 'implement backend' or 'add toggle' is not an executable leaf.

## Coverage and completion

| Requirement | Producer checks | Combined proof |
|---|---|---|
| R1–R4 | A1/AT1/AT2 | V1/VT1 and E1/ET1 |
| R5–R6 | U1/UT1 | V1/VT1 and E1/ET1 |
| R7 | A.AT3 and U.UT2 | V.VT1 and E.ET1 |

Bind the labels to returned canonical check IDs. Read back every saved specification, dependency, parent and requirement mapping. Complete P only after its planning checks pass; E/A/U/V remain open, unclaimed and unchecked at that point.

A real later checkpoint might say that API default/archive cases passed, malformed-query verification remains unrun, the current source/guide sections are named, and the next action is the exact error test. It cannot claim the hypothetical fixture above ran. The coordinator checkpoints the Session and releases its own claims when stopping; delegated roles report, not release. On resume consume that evidence before claiming ready work. Finish the Session only after actual integrated delivery and all current scope are reconciled.
