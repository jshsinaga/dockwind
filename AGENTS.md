# AGENTS.md

Strict rules for how to work in this project. These override default habits.
Domain specs live in `docs/`. Read the relevant ones before you start.

## Specs

- `DESIGN.md`: UI design patterns and components to follow.
- `docs/PRD.md`: what we build and why. Defines scope.
- `docs/ARCHITECTURE.md`: structure, layers, and conventions.
- `docs/ERD.md`: data model.

If a spec is missing, outdated, or conflicts with the request, ask. Don't guess.
Don't edit specs unless asked.

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

## 5. Surgical Changes

- Touch only what the request requires. Match the existing style.
- No drive-by refactors, renames, or reformatting.
- Clean up only the mess your own change created.
- If you spot unrelated issues, mention them. Don't fix them.

## 6. Goal-Driven Execution

- Define success criteria first. Verify with tests, a build, or a run before saying "done."
- For multi-step work, give a short plan with a check for each step.
- Report honestly: what changed, what's verified, and what isn't.

## Additional Rules

- Always use interactive questioning tools when asking the user, and state your recommended option.
