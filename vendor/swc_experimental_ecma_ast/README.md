Vendored from `swc_experimental_ecma_ast` 0.11.0, upstream revision
`57cf78c4bc1b963d4e4946492b88164958e6f764`.

Only the dependency manifest is patched: `unicode-id-start` 1.4.0 is pinned to
its upstream Git revision so Cargo can resolve it alongside tsrs's crates.io
1.1.2 pin. This preserves SWC's Unicode 17 tables. Rust sources are unchanged.
