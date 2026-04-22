# AI Agent Instruction Manual: Documentation Layer Guide

> **System Context:** This repository uses a four-stage planning chain. Treat `PRD.md`, `Architecture.md`, `TechSpec.md`, and `Tasks.md` as canonical artifacts with strict layer boundaries.

## The 4-Document Chain

1. **`PRD.md`** — conceptual product layer: problem, actors, glossary, capabilities, constraints, and scope boundaries
2. **`Architecture.md`** — logical system layer: containers, flows, resilience, and risks
3. **`TechSpec.md`** — physical implementation layer: stack, ADRs, state model, interfaces, structure, and verification contract
4. **`Tasks.md`** — execution layer: active critical path, current tasks, and archived completed work

**Authority flow:** PRD -> Architecture -> TechSpec -> Tasks

---

## Documentation Routing Table

| If you need to know... | Target File | Specific Section |
| --- | --- | --- |
| What product and scope Kraken serves | `PRD.md` | `1. Executive Summary`, `4. Functional Capabilities`, `6. Boundary Analysis` |
| Which term should be used consistently | `PRD.md` | `2. Ubiquitous Language (Glossary)` |
| What the logical boundaries are | `Architecture.md` | `1. Architectural Strategy`, `2. System Containers`, `4. Critical Execution Flows` |
| What concrete interfaces, state, and tests exist | `TechSpec.md` | `1. Stack Specification`, `3. State & Data Modeling`, `4. Interface Contract`, `5. Implementation Guidelines` |
| What should happen next | `Tasks.md` | `1. Executive Summary & Active Critical Path`, `4. Ticket List` |
| What was already delivered in the previous wave | `Tasks.md` | `Appendix A-C` |
| How CI and release gates currently work | `reports/GatePolicy.md` | all sections |

---

## Documentation Rules

1. **Respect layer boundaries.** Do not move stack or ABI detail into the PRD. Do not move product intent into TechSpec. Do not invent contracts in Tasks.
2. **Preserve continuity.** Version history, archived completed scope, operator preferences, and major historical decisions are part of the trust surface.
3. **Use current framework shape.** The canonical docs now follow the current stage skeletons; keep future revisions in that format.
4. **Treat code as Brownfield truth.** If a doc drifts from the source tree, reconcile explicitly instead of silently preserving stale future-tense language.
5. **Keep active and archived scope separate.** `Tasks.md` should not let completed execution masquerade as the active backlog.

---

## When Revising Docs

### Product-layer change
- Start with `PRD.md`
- Validate whether the requested change is really a scope change or only an implementation/architecture change

### Logical design change
- Confirm the PRD already authorizes the change
- Revise `Architecture.md` before touching `TechSpec.md`

### Implementation contract change
- Confirm Architecture already authorizes it
- Revise `TechSpec.md`
- Then revise `Tasks.md` if execution implications change

### Execution-plan change
- Only revise `Tasks.md` once the upstream contract is already present
- Preserve archived scope if it still explains current reality

---

## Current Repo-Specific Notes

- `TechSpec.md` is now a **current-state Brownfield spec**, not a future-phase memo.
- `Tasks.md` currently marks the active plan as intentionally idle until a new post-v4 backlog is ratified, while preserving the large archived v6/v4 delivery appendix.
- `reports/GatePolicy.md` reflects the current CI host test surface, including install smoke and runner tests.
