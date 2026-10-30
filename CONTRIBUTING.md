# Contributing to Nord

## Workflow

1. Start from an up-to-date `main` and create a focused branch for each change.
2. Keep reusable engine infrastructure in `crates/` and Nord-specific composition/gameplay in `src/`.
3. Run `./scripts/validate.ps1` and the relevant build script before opening a pull request.
4. Open a pull request to `main`; include the reason, validation results, and any compatibility or asset notes.
5. Wait for CI and review, resolve conversations, then merge using squash or rebase.

Do not force-push or rewrite commits already shared with collaborators unless the team has agreed to it. Use Git LFS for binary art/audio/video assets; never commit `target/` output.

## Commits

Use a short imperative subject that states the change, such as `Add chunk coordinate types`. Keep each commit focused and make sure it builds when practical.
