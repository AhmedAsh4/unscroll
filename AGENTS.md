# Unscroll agent workflow

Optimize for low wall time and token use without weakening correctness, security, licensing, review, or acceptance evidence.

## Default model

- Use `gpt-5.6-terra` with medium reasoning for implementation and ordinary code review.
- Increase `gpt-5.6-terra` to high reasoning only when focused evidence shows the task needs deeper analysis than medium provides.
- Use `gpt-5.6-sol` with high reasoning only for one scoped review when the task's core work is architecture, security, legal/licensing, or final-release acceptance; do not run the whole implementation on it unless the work cannot be safely decomposed.
- Do not upgrade models merely because a task is large; narrow the context first.

## Start and scope

- Reuse the named worktree. First inspect status, branch, and dependency HEAD; preserve existing work.
- Read this file, the complete approved spec, and the complete plan once per session. Their approval satisfies the design gate: do not invoke brainstorming, redesign the task, or request renewed design approval.
- Execute only the requested numbered task. Do not pull the next task forward.
- Preserve earlier decisions and touch only task-owned files. Do not push, merge, publish, or open a PR unless explicitly requested.

## Context and tools

- Give subagents a narrow brief containing the task, owned files, invariants, acceptance checks, and dependency SHA. Use `fork_turns: "none"`; do not copy the conversation or entire documents into agent prompts.
- Use CodeGraph for structural questions and `rg` for literal text. Do not repeat CodeGraph results with grep or delegate repository exploration already answered by CodeGraph.
- Read specific files or bounded ranges. Never print whole lockfiles, generated output, binaries, licenses, vendor notices, or large diffs when hashes, headers, summaries, or filtered lines prove the point.
- Inspect `git diff --stat` and changed filenames before opening a scoped diff. Avoid repeatedly rendering the same diff or report.
- Prefer native tools, standard libraries, and already-installed dependencies. Add no dependency for convenience alone.

## Implementation and review

- Use the `using-git-worktrees` (reuse the named worktree), `subagent-driven-development`, `test-driven-development`, `requesting-code-review`, and `verification-before-completion` workflows for every numbered implementation task.
- Before editing, translate the task requirements and verification into a closed acceptance checklist. Do not send the candidate to review until every item passes or is explicitly unavailable.
- For plan execution, use one fresh implementer. The implementer owns the complete task, TDD, and focused tests, and returns only when complete or genuinely blocked; omit intermediate status turns.
- After implementation, use one fresh reviewer with the dependency-to-candidate diff and closed acceptance checklist. The reviewer returns all in-scope findings in one consolidated report and does not rebuild or rediscover the repository.
- Return the complete finding batch to the same implementer in one correction round, then have the same reviewer perform one scoped re-review of the fixes and original acceptance criteria. Do not spawn replacement fix agents or ceremonial final, release, or closing reviewers.
- If scoped re-review finds an in-scope defect or regression, keep the same implementer and reviewer, batch the remaining fixes, and repeat only the focused proof needed for those findings.
- Parallelize only independent read-only work. Never run concurrent writers or duplicate builds in the shared worktree.
- Keep commentary to decisions, blockers, and material evidence; omit play-by-play narration and repeated summaries.

## Builds and verification

- Reuse downloaded artifacts, package caches, lockfiles, and prior build outputs. Do not clean caches or build trees unless the acceptance criterion explicitly tests a clean or reproducible operation.
- During implementation, run the smallest test that can fail for the current change. Expand only when a focused failure proves it necessary.
- Agents must not each run the full repository gate. After review fixes, the controller owns exactly one fresh full acceptance gate covering the task plus required earlier-task regressions.
- Do not rerun a passing expensive command solely so another agent can report it. Record its command, exit status, and concise result once for controller use.
- On failure, diagnose with the smallest reproducer and rerun the failed/focused check before returning to the full gate.
- If required device-runtime proof is unavailable, record it once as unavailable. Packaging is not runtime proof, and unavailable hardware does not justify repeated builds.
- Use locked dependency modes. Avoid network refreshes when pinned cached inputs suffice.
- Keep generated binaries and downloads ignored unless the task and repository policy explicitly require tracking them.

## Completion and handoff

- Before committing, audit changed filenames, task checkboxes, unapproved binaries, unrelated changes, and exact commit text.
- After the single full gate, commit once with the task's exact message and confirm the tracked worktree is clean.
- Report only the commit SHA, material changes, verification results, and real limitations.
- The next-task prompt contains only: worktree/branch, actual dependency SHA, spec/plan paths, task number and title, owned files, task-specific requirements, verification, acceptance, exact commit message, and the recursive handoff rule. Do not repeat this workflow or model guidance in generated prompts.
- Generate compact handoffs through Task 21. Task 21 generates no Task 22 prompt.
