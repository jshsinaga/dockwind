# AGENTS.md

Strict rules for how to work in this project. These rules override default habits.
This file is the entry point. It points to the canonical specs and does not repeat
their content.

## Read First

| File | Authority |
|---|---|
| [docs/PRD.md](docs/PRD.md) | Scope, requirements, acceptance criteria, and feature status |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) | Module ownership, data flow, thread and lifetime rules |
| [DESIGN.md](DESIGN.md) | UI and window patterns |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | Config fields, validation, reload, and save compatibility |
| [docs/RELEASE.md](docs/RELEASE.md) | Versioning, installer, and hosted CI |
| [docs/engineering.spec.md](docs/engineering.spec.md) | Rust conventions and verification rules |
| [docs/HANDOFF.md](docs/HANDOFF.md) | Current state, pending work, and dated evidence |
| [docs/decisions.md](docs/decisions.md) | Accepted decisions D001 onward |
| [docs/implementation_plan.md](docs/implementation_plan.md) | Historical original plan. Not current scope |
| [README.md](README.md) | User-facing setup and behavior |

Dockwind has no database. `docs/ERD.md` does not apply. The configuration schema
is in `docs/CONFIGURATION.md`.

If a spec is missing, outdated, or conflicts with the request, report the conflict.
Do not guess. Do not edit specs unless asked.

Before Rust work, read the globally installed `rust-best-practices` skill by its
portable name. Upstream is `apollographql/skills`. Local rules in this repository
win over generic advice. For agent-facing docs, read the `asd-ste100` and
`writing-for-agents` skills.

## 1. Think Before Coding

- State assumptions. If the request is ambiguous, ask instead of choosing silently.
- If there are multiple interpretations, list them.
- If a simpler approach exists, say so before building the more complex one.

## 2. Simplicity (KISS + YAGNI)

- Write the minimum code that solves the request. Nothing speculative.
- Do not implement unrequested features, hooks, config options, flags, or extras.
- No abstractions for single-use code.
- Prefer simple, readable logic over design patterns or deep abstractions.
- No error handling for scenarios that can't happen.

## 3. Reuse (DRY)

- Search the codebase before writing a utility or helper.
- Reuse existing service layers. Don't duplicate logic.
- Extract on the third repeat, not before.

## 4. No Workarounds, No Fallbacks

- Fix the root cause. No hacks.
- No silent fallbacks, swallowed errors, or default values that mask failures.
- Fail loudly. If blocked, report it. Don't route around it.
- Never make tests pass by special-casing them.
- Logged degradation for invalid config and unsupported blur is the only allowed
  exception. It is explicit and documented in `docs/CONFIGURATION.md`.

## 5. Surgical Changes

- Touch only what the request requires. Match the existing style.
- No drive-by refactors, renames, or reformatting.
- Clean up only the mess your own change created.
- If you spot unrelated issues, mention them. Don't fix them.

## 6. Goal-Driven Execution

- Define success criteria first. Verify with tests, a build, or a run before saying
  "done."
- For multi-step work, give a short plan with a check for each step.
- Report honestly: what changed, what's verified, and what isn't.

## 7. Verification and Evidence

- Run `scripts/check.ps1`. It works from any directory. It runs formatting, check,
  clippy, tests, the release build, and local Markdown link checks.
- A pure-logic change needs a unit test.
- A GUI or Win32 change needs a manual verification step. Record the date, the
  environment, and the result in `docs/HANDOFF.md`.
- Do not claim a behavior is verified when only the code path exists.
- Do not enable new warnings or denies in `Cargo.toml` without coordinating the
  native and test implications. Do not add broad lint allowances.

## 8. Documentation Discipline

- Update the canonical spec when behavior, ownership, or configuration changes.
  Do not copy the same fact into several documents.
- Record a deviation in `docs/decisions.md`.
- Keep `docs/HANDOFF.md` current for state and pending work. It is not an
  architecture document.
- Use plain ASCII punctuation in project text.

## Additional Rules

- Always use interactive questioning tools when asking the user, and state your
  recommended option.
- No git commits, pushes, publishes, or spec-plan files unless assigned.
