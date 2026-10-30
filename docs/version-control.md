# Version control

## Current repository setup

Nord uses Git at the repository root. Rust source, manifests, documentation, build scripts, and `Cargo.lock` are tracked. Cargo output under `target/`, common IDE state, OS metadata, editor swap files, and logs are ignored. Do not commit generated executables or build caches.

The default local branch is `main`. Work should be grouped into focused commits with messages that describe the change. Keep commits reviewable; avoid combining unrelated engine, game, and asset changes.

## Text and binary files

`.gitattributes` makes Git normalize text to LF in the repository so Windows and Unix checkouts produce stable diffs. Common game binary assets (textures, 3D source files, voxel models, audio, and video) use Git LFS. Install Git LFS on every machine that clones or contributes assets, and verify it with `git lfs version`. LFS stores pointer files in Git while the binary content is stored separately by the Git hosting provider.

The canonical remote is `origin` at `https://github.com/N0C4ps/Nord`. Pushes share commits and create an off-machine copy of Git history. A weekly GitHub Actions backup stores a full Git bundle and downloaded LFS objects as artifacts for 90 days; run it manually from Actions when an immediate snapshot is needed. Restore by downloading both artifacts, running `git clone nord-repository.bundle restored-repo`, and placing the archived LFS objects in `.git/lfs/objects` before `git lfs checkout`.

GitHub LFS storage and bandwidth are quota-metered. Track only source assets that need versioning; keep generated build output in `target/` and don't use LFS as an artifact cache.

## History and collaboration

Commits create local history; pushing to `origin` shares and backs up Git history. Branch protection for `main` requires pull requests, one approving review, passing CI, resolved conversations, linear history, and prevents force-pushes/deletion. Dependabot checks Cargo and workflow dependencies weekly.

Avoid rewriting shared history. For changes that have already been pushed and reviewed, prefer follow-up commits over `reset` or force-push. Use feature branches or isolated Git worktrees for parallel changes once the team workflow is established.

## Maintenance

Git performs automatic maintenance as the repository grows. `git maintenance run` can be used for explicit repository optimization; background scheduling should be enabled only when this machine is intended to maintain the repository regularly. Do not run aggressive garbage collection during an active Git operation.

## Research references

- [Git attributes](https://git-scm.com/docs/gitattributes): text normalization, line endings, and binary treatment.
- [Git FAQ](https://git-scm.com/docs/gitfaq): text/binary classification and artifact storage guidance.
- [Git maintenance](https://git-scm.com/docs/git-maintenance): optimization tasks and scheduled maintenance.
- [Git ignore](https://git-scm.com/docs/gitignore): ignored generated and local-only files.
