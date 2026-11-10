# Engineering Spec

Status: active. This document defines Rust conventions and verification rules for
Dockwind. It does not define product scope, architecture, or current state.

- Workflow rules: `AGENTS.md`
- Scope and acceptance criteria: `docs/PRD.md`
- Module ownership and lifetimes: `docs/ARCHITECTURE.md`
- Config schema: `docs/CONFIGURATION.md`
- Current state and evidence: `docs/HANDOFF.md`

## 1. Toolchain and Dependencies

| Item | Rule |
|---|---|
| Edition | Rust 2024 |
| Toolchain | Pinned to the verified release in `rust-toolchain.toml` |
| Target | `x86_64-pc-windows-msvc` |
| Lockfile | Commit `Cargo.lock`. Verification uses `--locked` |
| GPUI | Exact pin `=0.2.2`. No wildcard, range, or moving branch |
| `windows` crate | 0.61, aligned with the version GPUI uses. Check with `cargo tree -d` |
| New dependencies | Only when the current request needs them. Record the reason in `docs/decisions.md` if the choice is not obvious |

Do not update dependency versions as part of an unrelated change. Do not replace
the UI stack without an approved decision.

## 2. Module Boundaries

- Follow the target layout in `docs/ARCHITECTURE.md`. Do not create empty feature
  directories.
- Keep pure logic independent of GPUI and Win32. This includes config parsing and
  validation, theme derivation, window filtering, app grouping, auto-hide state,
  dock geometry, and hotkey parsing.
- Keep all `unsafe` and raw handle work under `src/win/`.
- Keep UI state on the GPUI thread. Cross-thread input arrives through channels.
- `src/win/` stays a flat platform boundary. Do not turn it into a feature layer.

## 3. Error Handling

- Use `anyhow::Result` at binary and application boundaries.
- Use `thiserror` for typed config or domain errors that callers inspect.
- Propagate with `?`. Do not convert an error into a silent default.
- Do not use `unwrap()` or `expect()` in production paths. `build.rs` and tests
  are the current exceptions. Do not add new production uses.
- Panic only for a violated internal invariant that cannot be reported. Prefer a
  returned error.
- Invalid config and unsupported blur are explicit, logged degradations. They are
  defined in `docs/CONFIGURATION.md`. No other silent fallback is allowed.
- A failed save, restore, or native call must be logged or returned. Do not
  swallow it with `let _ =`.
- Do not special-case a test to make it pass.

## 4. Unsafe and Native Code

- Wrap `unsafe` in a small safe function.
- Put a `// SAFETY:` comment immediately before each unsafe block. State the
  pointer, handle, lifetime, or thread condition that makes it valid.
- Check handles for null or invalid values before use.
- Convert `HWND`, `HICON`, and other handles to plain values at the boundary.
- Keep recovery and cleanup paths explicit. `TaskbarGuard`, the panic hook, the
  console handler, and `WM_ENDSESSION` must stay independent.
- Do not add a broad `#[allow(...)]`. Prefer `#[expect(...)]` with a reason when a
  lint is a true false positive.

## 5. Linting

Baseline:

```powershell
cargo clippy --all-targets --locked -- -D warnings
```

Rules:

1. Fix the warning. Do not silence it.
2. If a local exception is unavoidable, use `#[expect(clippy::lint_name)]` and
   state why.
3. Do not add crate-wide allowances.
4. Native SAFETY coverage, `unwrap` and `expect` in tests, and `unwrap` and
   `expect` in `build.rs` have different constraints. Coordinate before enabling
   further warnings or denies in `Cargo.toml`.
5. Keep the existing lockfile and dependency versions unless the request changes
   them.

### 5.1 Active targeted lint policy

The integrator confirmed native coverage. These lints are active:

| Scope | Lint | Level |
|---|---|---|
| `[lints.rust]` | `unsafe_op_in_unsafe_fn` | `deny` |
| `[lints.clippy]` | `undocumented_unsafe_blocks` | `deny` |

Do not add global `unwrap` or `expect` denies. Tests and `build.rs` use them as
permitted fatal errors. Production-only `clippy::unwrap_used` and
`clippy::expect_used` are denied at crate scope with `cfg(not(test))` in
`src/main.rs`. Do not relax a lint to pass the gate. Report a lint failure to the
integrator.

## 6. Testing

- Add a unit test for every pure-logic change.
- Test behavior, not implementation details. Prefer one behavior per test.
- Use descriptive names. A name may state the condition and the expected result.
- Use table-driven tests when the same rule covers several inputs.
- Keep tests independent. Do not depend on a previous test's state.
- Use temporary directories for file tests.
- Do not test GPUI rendering or Win32 side effects through a mock that hides the
  real contract. Use a manual verification step and record it.
- A test that needs a real desktop, DPI, Explorer restart, or fullscreen app is a
  manual check, not a unit test.

## 7. Verification

`scripts/check.ps1` is the canonical local gate. It resolves the repository root
from its own path, so it works from any directory.

The script runs, in order:

```powershell
cargo fmt --check
cargo check --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
```

It stops at the first failed native command and reports that command and its exit
code. It then checks local Markdown file links. The parser tracks fence character
and run length, masks multiline code spans, and parses balanced labels and
destinations. It skips external, plugin, and skill URIs. It ignores anchors when
checking the file part. It does not create files to satisfy historical examples.
The checker runs a built-in self-test with positive and negative cases before it
scans the repository.

Verification rules:

- A change is not done until the gate passes, or the unverified part is reported.
- A GUI or Win32 claim needs a dated manual check with the environment and result.
- Do not claim "verified" from a code read alone.
- Do not hide a failure caused by another worker's in-progress edit. Coordinate
  and report it.
- The final full gate passed on 2026-11-10. Later runs apply to the changes that
  followed, not to that source.
- Hosted CI is deferred until a host is selected. Do not invent a CI provider.

## 8. Comments and Documentation

- Use `//` comments for why: safety, workarounds, platform behavior, assumptions.
- Use `///` doc comments for public APIs and non-obvious contracts.
- Do not write a comment that restates the next line.
- Do not leave a `// TODO` without a tracked issue or a decision reference. If
  no issue tracker exists, record the item in `docs/HANDOFF.md` instead.
- Use plain ASCII punctuation in project text. Use `->` or "to" instead of arrows.
- Keep one source of truth. Link to the canonical doc instead of copying it.
- When behavior changes, update the matching spec in the same change.

## 9. Change Discipline

1. Trace the change to a requirement, an accepted unfinished feature, or an
   explicit user request.
2. State assumptions before implementation.
3. Check existing helpers before adding a new one.
4. Keep the diff surgical. Match the surrounding style.
5. Record a deviation in `docs/decisions.md` with the reason and trade-off.
6. Update `docs/HANDOFF.md` with state and dated evidence.
7. Report what changed, what passed, and what remains unverified.

## 10. Definition of Done

A change is done when:

1. The scope is traceable to `docs/PRD.md`, a decision, or an explicit request.
2. Pure logic has a test, and native or GUI behavior has a manual check or an
   explicit unverified note.
3. `scripts/check.ps1` passes, or the blocked part is reported honestly.
4. No error is swallowed and no silent fallback is added.
5. The canonical docs match the behavior.
6. The final report lists changed paths, tests, blockers, and deviations.
