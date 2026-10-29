# SWC conformance

Generate parser diagnostics with V8 and tsrs, and SWC resolver reference snapshots.
Requires Rust 1.99.1, CMake, and a C/C++ toolchain.

```sh
./scripts/clone_fixtures.sh
cargo run --locked --release -p fixture-json
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

The workspace contains two crates:

- `fixture-json` decodes upstream fixtures into `fixtures-json/`, including
  positive and negative cases, Test262 metadata and modes, TypeScript virtual
  file line indices, configuration variants, and baseline diagnostic codes.
  It also prepares the fixed snapshot selections in `fixtures-json/runners/`.
  Source text stays in the upstream fixtures.
- `swc-conformance`, in `crates/swc-conformance/`, loads those selections and
  reconstructs source from `fixtures/`, then generates snapshots. Case filtering,
  mode expansion, and snapshot naming are performed by the generator.

Each runner uses fixed cases: V8 reads Test262 parse negatives, tsrs reads
TypeScript syntax-error variants, and SWC reads Test262 language cases and SWC
minifier issues. Runners return JSON results; the snapshot CLI handles all
writes. Resolver JSON maps identifier end offsets to canonical binding IDs
(zero means unresolved).

The snapshot CLI in `crates/swc-conformance/src/main.rs` dispatches all updates.
The local SWC AST patch in `vendor/` pins its Unicode dependency to a Git revision,
allowing it to
coexist with tsrs's older crates.io dependency while preserving both versions'
identifier classification.

Validate changes with full snapshot generation and output comparisons, plus
workspace Cargo check/Clippy and formatting. Do not add unit tests.

The script prepares Test262, TypeScript, Babel, ESTree conformance, and SWC under
`fixtures/`, using the revisions previously pinned by SWC Next. SWC Next passes
its fixture directory and shared cache directory. Pins are maintained in the script.
Bare repositories are cached under the Git common directory for worktree reuse.
Run setup serially across worktrees sharing a cache.
