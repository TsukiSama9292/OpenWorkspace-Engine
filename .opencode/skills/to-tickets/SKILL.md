---
name: to-tickets
description: Break a plan, spec, or the current conversation into a set of tracer-bullet tickets, each declaring its blocking edges, published to the configured tracker — edges as text in one file per ticket locally, or native blocking links on a real tracker.
disable-model-invocation: true
---

# To Tickets

Break a plan, spec, or conversation into a set of **tickets** — in this repo, exactly three: one backend, one frontend, one integration test.

The issue tracker and triage label vocabulary should have been provided to you — run `/setup-matt-pocock-skills` if not.

## Process

### 1. Gather context

Work from whatever is already in the conversation context. If the user passes a reference (a spec path, an issue number or URL) as an argument, fetch it and read its full body and comments.

### 2. Explore the codebase (optional)

If you have not already explored the codebase, do so to understand the current state of the code. Ticket titles and descriptions should use the project's domain glossary vocabulary, and respect ADRs in the area you're touching.

Look for opportunities to prefactor the code to make the implementation easier. "Make the change easy, then make the easy change."

### 3. Draft the tickets

Break the work into **tracer bullet** tickets.

<vertical-slice-rules>

- Each slice cuts a narrow but COMPLETE path through every layer (schema, API, UI, tests) — vertical, NOT a horizontal slice of one layer
- A completed slice is demoable or verifiable on its own
- Each slice is sized to fit in a single fresh context window
- Any prefactoring should be done first

</vertical-slice-rules>

**In this repo, split the feature into exactly three tickets — one backend, one frontend, one end-to-end test.** The user finds ticket fan-out and subagent dispatch too complex for this codebase; keep it to one continuous pass per ticket, done directly:

1. **`01-be-<slug>` (backend)** — all Rust API work: migrations, policy/domain logic, route guardrails, API contracts, and their tests. Backend-only concerns (migration mechanics, backfills, contract drops) all live here. Lands with the full API suite green and zero compiler warnings.
2. **`02-fe-<slug>` (frontend)** — all SvelteKit UI work: contract types/API client, stores, components, and their tests. Built against the backend schema it just received. Layout/visual consistency with the existing design language is part of its scope. Lands with `svelte-check` and the Vitest suite green.
3. **`03-int-end-to-end` (integration)** — the full-stack test verifying both sides work together against the real backend.

Rules:

- **One continuous pass per ticket, no subagents.** The backend ticket is completed in its entirety before the frontend ticket begins; the integration ticket runs last. Work top to bottom; there is no frontier.
- **Blocking is sequential.** `01-be` has no blockers. `02-fe` is blocked by `01-be`. `03-int` is blocked by `02-fe`.
- **Each ticket is self-contained and green on its own** — that is what lets the next ticket start from a known-good base.

**Wide refactors are the exception to vertical slicing.** A **wide refactor** is one mechanical change — rename a column, retype a shared symbol — whose **blast radius** fans across the whole codebase, so a single edit breaks thousands of call sites at once and no vertical slice can land green. Don't force it into a tracer bullet; sequence it as **expand–contract**. First expand: add the new form beside the old so nothing breaks. Then migrate the call sites over in batches sized by blast radius (per package, per directory), each batch its own ticket blocked by the expand, keeping CI green batch to batch because the old form still exists. Finally contract: delete the old form once no caller remains, in a ticket blocked by every migrate batch. When even the batches can't stay green alone, keep the sequence but let them share an integration branch that all block a final integrate-and-verify ticket — green is promised only there.

### 4. Quiz the user

Present the proposed breakdown as a numbered list — three tickets. For each ticket, show:

- **Title**: short descriptive name
- **Blocked by**: which other tickets (if any) must complete first
- **What it delivers**: the end-to-end behaviour this ticket makes work

Ask the user:

- Is the three-ticket split right — does the backend ticket cover all API work, the frontend ticket all UI work?
- Is the scope of each ticket complete — anything missing, anything that belongs in a different ticket?
- Are the blocking edges correct?

Iterate until the user approves the breakdown.

### 5. Publish the tickets to the configured tracker

Publish the approved tickets. **How** depends on the tracker `/setup-matt-pocock-skills` configured — the tickets are the same either way, only the shape of the blocking edges changes:

- **Local files** → write one file per ticket under `.scratch/<feature-slug>/issues/<NN>-<slug>.md`, numbered `01` (backend), `02` (frontend), `03` (integration): e.g. `01-be-<slug>`, `02-fe-<slug>`, `03-int-end-to-end`. Blockers always carry a lower number: `01-be` has none, `02-fe` is blocked by `01-be`, `03-int` is blocked by `02-fe`. Each file's "Blocked by" lists the numbers/titles it depends on. Use the per-ticket file template below — one ticket per file, never a single combined file.
- **A real issue tracker (GitHub, Linear, …)** → publish one issue per ticket in dependency order (blockers first) so each ticket's blocking edges can reference real identifiers. Use the platform's native blocking / sub-issue relationship where it has one; otherwise set each ticket's "Blocked by" to the blocking issues. Apply the `ready-for-agent` triage label unless instructed otherwise — the tickets are agent-grabbable by construction.

Work the tickets **top to bottom**: `01-be` first, then `02-fe` (blocked by `01-be`), then `03-int` (blocked by `02-fe`). There is no frontier — the chain is strictly sequential, one continuous pass per ticket.

Do NOT close or modify any parent issue.

<local-ticket-template>

# <NN> — <Ticket title>

**Track:** backend / frontend / integration

**What to build:** the end-to-end behaviour this ticket makes work, from the user's perspective — not a layer-by-layer implementation list.

**Blocked by:** the numbers/titles of the tickets that gate this one (default: `01-be` has none, `02-fe` blocked by `01-be`, `03-int` blocked by `02-fe`), or "None — can start immediately".

**Status:** ready-for-agent

- [ ] Acceptance criterion 1
- [ ] Acceptance criterion 2

</local-ticket-template>

<issue-template>

## Parent

A reference to the parent issue on the tracker (if the source was an existing issue, otherwise omit this section).

## Track

backend / frontend / integration

## What to build

The end-to-end behaviour this ticket makes work, from the user's perspective — not layer-by-layer implementation.

## Acceptance criteria

- [ ] Criterion 1
- [ ] Criterion 2

## Blocked by

- A reference to each blocking ticket, or "None — can start immediately".

</issue-template>

In either form, avoid specific file paths or code snippets — they go stale fast. Exception: if a prototype produced a snippet that encodes a decision more precisely than prose can (state machine, reducer, schema, type shape), inline it and note briefly that it came from a prototype. Trim to the decision-rich parts — not a working demo, just the important bits.
