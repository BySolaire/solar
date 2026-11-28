# Upstream reference source

This directory is a verbatim copy of the TypeScript engine of
[open-bot-avatar](https://github.com/DominikScholz/open-bot-avatar) by Dominik Scholz,
`packages/core/src/*.ts` (the `dom/` folder excluded), at commit `e10afe4`
(`e10afe49d46da8ffcc6545f5d6e8ba1de4ea8c5d`, package version 0.5.2).

It is MIT licensed (see `LICENSE`, Copyright (c) 2026 Dominik Scholz). The Rust crate in
`../src` is a port of it; the golden-test generator (`../tests/golden/gen.mjs`) imports these
files with Node to produce the reference fixtures. Do not edit them: they are the spec.
