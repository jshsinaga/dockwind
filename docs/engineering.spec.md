# Engineering Spec

Extracted from `AGENTS.md` and adapted to the current Dockwind project state.

- Source: https://github.com/jshsinaga/agents.md (`AGENTS.md`)
- Extracted: 2026-11-10
- Status: active
- Authority: `AGENTS.md` remains authoritative. This document applies its rules to this repository and does not replace it.

The source contains engineering-workflow rules, not framework API rules. The enforceable rules are extracted in section 2 and mapped to the Dockwind stack. Project constraints that shape those rules are in section 3.

## 1. Current Project State

See `docs/HANDOFF.md` for the authoritative current state, architecture,
verification evidence, runtime safety rules, and remaining work.

| Item | State |
|---|---|
| Code | Rust binary crate with `src/`, `assets/`, and 36 passing tests. |
| Existing docs | `AGENTS.md`, `README.md`, `docs/HANDOFF.md`, `docs/implementation_plan.md`, `docs/decisions.md`, `docs/spike-m0.md`, and this spec. |
| VCS | Git repository on `main`; M0 through M7 milestone commits exist. |
| Product | Dockwind, a floating Windows 10/11 dock bar written in Rust. |
| UI stack | GPUI 0.2.2, pinned exactly; Windows-only native integration. |
| System integration | `windows` crate 0.61, matching GPUI's Windows dependency. |
| Delivery order | M0 through M6 complete; M7 implemented except AppBar, middle-click, tooltips, and UWP identity. |

`docs/implementation_plan.md` remains at its current path and is the scope source.
`docs/HANDOFF.md` records the current implementation state and next steps. No files
were moved when the documentation was updated.

### Missing specs referenced by AGENTS.md

`DESIGN.md`, `docs/PRD.md`, `docs/ARCHITECTURE.md`, and `docs/ERD.md` do not exist.

Do not create, rename, move, or delete files to satisfy those references without
user approval. Until the user decides how to map them, use
`docs/implementation_plan.md` for scope and `docs/HANDOFF.md` for current
architecture and state. If a task needs one of the missing specs, stop and ask.

## 2. Extracted Rules and Local Application

| ID | Rule from AGENTS.md | Application in Dockwind |
|---|---|---|
| E1 | Think before coding. State assumptions. If ambiguous, ask instead of choosing silently. List multiple interpretations. Mention a simpler approach before building a complex one. | State which milestone or plan section a change serves. Known unknowns must be resolved from evidence, not memory. GPUI capabilities were verified in M0; for future GPUI changes, verify against the pinned source. Do not choose a fallback UI stack without approval. |
| E2 | Simplicity (KISS + YAGNI). Minimum code. No unrequested features, hooks, config options, flags, or extras. No abstractions for single-use code. No handling for scenarios that cannot happen. | Implement only the current milestone. Do not add config keys, commands, UI features, or abstractions beyond `docs/implementation_plan.md`. v1 does not replace Start, system tray, notification center, or notification badges. Windows 10 blur is outside the MVP. |
| E3 | Reuse (DRY). Search before writing a utility or helper. Reuse existing service layers. Do not duplicate logic. Extract on the third repeat, not before. | Search `src/config.rs`, `src/theme.rs`, `src/app_state.rs`, and `src/win/` before adding helpers. Reuse the Win32 wrappers and pure-logic modules instead of duplicating handle or DPI logic. |
| E4 | No workarounds, no fallbacks. Fix the root cause. No silent fallbacks, swallowed errors, or defaults that mask failures. Fail loudly and report blockers. Never special-case tests. | Do not ship silent taskbar-restore failures, swallowed Win32 errors, or undocumented defaults. Explicit, logged degradation is allowed only where the plan requires it: invalid config uses the last valid value or defaults with a warning; unsupported blur falls back to `none` with an info log. If M0 fails, stop, write `docs/spike-m0.md`, and report. Do not silently switch UI frameworks. |
| E5 | Surgical changes. Touch only what the request requires. Match existing style. No drive-by refactors, renames, or reformatting. Clean up only your own mess. Report unrelated issues without fixing them. | Keep diffs scoped to the milestone or request. Match the Rust and Win32 patterns established in `src/win/`. Do not move existing docs, rename plan files, or refactor unrelated modules. Report unrelated defects instead of fixing them in the same change. |
| E6 | Goal-driven execution. Define success criteria first. Verify with tests, a build, or a run before saying done. Give a short plan with a check per step. Report what changed, what was verified, and what was not. | Use the milestone completion criteria and manual verification steps in `docs/implementation_plan.md`. Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo build --release`. A milestone is not done until its criteria are met. Keep one commit per logical milestone or fix. |

## 3. Project Constraints Used for Adaptation

These constraints come from `docs/implementation_plan.md` and are the current basis for applying the rules above.

1. Target is Windows 10/11, binary crate `dockwind`, target `x86_64-pc-windows-msvc`, Rust edition 2024.
2. GPUI must be pinned to an exact version or git revision in `Cargo.toml`. Do not use `*` or a moving branch. Align the `windows` crate version with the one GPUI uses; check with `cargo tree -d`.
3. Verify GPUI API names against the pinned source. Do not invent function names.
4. All Win32 code and `unsafe` belong under `src/win/`. Wrap `unsafe` in small safe functions with `// SAFETY:` comments.
5. Keep pure logic independent of Win32 so it can be tested: config parsing and validation, theme derivation, window filtering, app grouping, auto-hide state machine, dock rectangle and DPI calculations, hotkey parsing.
6. The M0 gate has passed. Evidence is in `docs/spike-m0.md`; do not repeat the feasibility spike unless the UI stack changes.
7. Taskbar hiding is only performed through `TaskbarGuard`, with the restore command and recovery handlers in place. Keep `--restore-taskbar` first in `main`.
8. The native taskbar must always be restored on normal shutdown, panic, `WM_ENDSESSION`, and via the standalone restore command. Explorer restart must not leave the taskbar hidden.
9. Invalid config values are logged and replaced by the last valid value or defaults. The application must not crash or silently ignore the error.
10. Quality gates are required per milestone: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo build --release`.
11. Read `docs/HANDOFF.md` at the start of a new session. It is the authoritative current-state and continuation guide.

## 4. Definition of Done for Any Change

1. Scope is traced to a milestone in `docs/implementation_plan.md` or to an explicit user request.
2. Assumptions and interpretations are stated before implementation.
3. The diff is minimal, and existing helpers were checked first.
4. No failure is swallowed. Any permitted degradation is explicit in the plan and logged.
5. Pure logic has unit tests. GUI and Win32 behavior has a manual verification step.
6. Required quality gates pass, or the unverified parts are reported as unverified.
7. The final report states what changed, what was verified, and what remains open.

## 5. Conflict Handling

- `AGENTS.md` overrides default habits.
- `docs/implementation_plan.md` is the current scope source until the missing specs are resolved.
- If a generic rule conflicts with an explicit plan behavior, ask before coding.
- The plan's invalid-config behavior is an explicit, logged fallback, so it does not violate the ban on silent fallbacks.
- If a task needs `DESIGN.md`, `docs/PRD.md`, `docs/ARCHITECTURE.md`, or `docs/ERD.md`, ask. Do not guess or create replacement files.

## 6. Change Log

| Date | Change |
|---|---|
| 2026-11-10 | Created from `AGENTS.md` and adapted to the Dockwind implementation plan. No files moved. |
| 2026-11-10 | Updated current state after M0-M7 implementation and added `docs/HANDOFF.md` as the continuation guide. |
