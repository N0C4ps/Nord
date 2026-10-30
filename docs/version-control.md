# Version control

## Current repository setup

Nord uses Git at the repository root. Rust source, manifests, documentation, build scripts, and `Cargo.lock` are tracked. Cargo output under `target/`, common IDE state, OS metadata, editor swap files, and logs are ignored. Do not commit generated executables or build caches.

The default local branch is `main`. Work should be grouped into focused commits with messages that describe the change. Keep commits reviewable; avoid combining unrelated engine, game, and asset changes.

## Text and binary files

`.gitattributes` makes Git normalize text to LF in the repository so Windows and Unix checkouts produce stable diffs. Common game binary assets (textures, 3D source files, voxel models, audio, and video) use Git LFS. Install Git LFS on every machine that clones or contributes assets, and verify it with `git lfs version`. LFS stores pointer files in Git while the binary content is stored separately by the Git hosting provider.

The repository currently has no remote configured. LFS pointers can be committed locally, but binary content cannot be shared until a host with LFS support is configured. Keep large generated build outputs in `target/`, never in LFS.

## History and collaboration

Commits create local history; they do not back up or share it. Before team collaboration or remote backup, configure a repository host and origin, enable branch protection for `main`, require review and CI checks, and verify that the host supports Git LFS storage and bandwidth needs. These settings cannot be completed locally without choosing a host and organization.

Avoid rewriting shared history. For changes that have already been pushed and reviewed, prefer follow-up commits over `reset` or force-push. Use feature branches or isolated Git worktrees for parallel changes once the team workflow is established.

## Maintenance

Git performs automatic maintenance as the repository grows. `git maintenance run` can be used for explicit repository optimization; background scheduling should be enabled only when this machine is intended to maintain the repository regularly. Do not run aggressive garbage collection during an active Git operation.

## Research references

- [Git attributes](https://git-scm.com/docs/gitattributes): text normalization, line endings, and binary treatment.
- [Git FAQ](https://git-scm.com/docs/gitfaq): text/binary classification and artifact storage guidance.
- [Git maintenance](https://git-scm.com/docs/git-maintenance): optimization tasks and scheduled maintenance.
- [Git ignore](https://git-scm.com/docs/gitignore): ignored generated and local-only files.
