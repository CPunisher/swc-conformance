# SWC conformance

Generate parser and resolver reference snapshots, and manage pinned fixtures for
SWC Next conformance tests.

```sh
./scripts/clone_fixtures.sh
cargo run --locked --release -- parser
cargo run --locked --release -- resolver
```

The script prepares Test262, TypeScript, Babel, ESTree conformance, and SWC under
`fixtures/`, using the revisions previously pinned by SWC Next. SWC Next passes
its fixture directory and shared cache directory. Pins are maintained in the script.
Bare repositories are cached under the Git common directory for worktree reuse.
Run setup serially across worktrees sharing a cache.
