# SWC conformance

Generate parser diagnostics with V8 and tsrs, and SWC resolver reference snapshots.
Requires Rust 1.99.1, CMake, and a C/C++ toolchain.

```sh
./scripts/clone_fixtures.sh
cargo run --locked --release -- swc
cargo run --locked --release -- v8
cargo run --locked --release -- tsrs
```

Parser snapshots contain only expected-error cases: Test262 parse negatives in
their script/strict/module modes, and TypeScript baseline variants with parser,
scanner, binder, or grammar diagnostics. TypeScript type errors are filtered out.
JSON files under `tests/parser/` retain codes, related locations, and UTF-8 byte
offsets into the prepared input. Diagnostic message text is omitted. An empty
diagnostic list records an unexpectedly accepted case. Each suite records its
engine version.

Case readers prepare fixture inputs and options. Each runner uses fixed cases:
V8 reads Test262 parse negatives, tsrs reads TypeScript baseline variants, and
SWC reads Test262 language cases and SWC minifier issues. Runners return JSON
results; `src/main.rs` handles all snapshot writes. Resolver JSON maps
identifier end offsets to canonical binding IDs
(zero means unresolved).

`src/main.rs` dispatches all snapshot updates in one binary. The local SWC AST
patch in `vendor/` pins its Unicode dependency to a Git revision, allowing it to
coexist with tsrs's older crates.io dependency while preserving both versions'
identifier classification.

Validate changes with full snapshot generation and output comparisons, plus
Cargo check/Clippy and formatting. Do not add unit tests.

The script prepares Test262, TypeScript, Babel, ESTree conformance, and SWC under
`fixtures/`, using the revisions previously pinned by SWC Next. SWC Next passes
its fixture directory and shared cache directory. Pins are maintained in the script.
Bare repositories are cached under the Git common directory for worktree reuse.
Run setup serially across worktrees sharing a cache.
