# Painter cross-check against a browser canvas

The golden tests prove the frames equal the TypeScript engine's. This checks the painter: the
same frames drawn by a browser with the canvas code of `dom/paint.ts` (flat branch) and by
`bot_avatar::paint`, compared pixel by pixel. It needs a browser, so it is not part of
`cargo test`.

```sh
node crates/bot-avatar/tests/golden/canvas/frames.mjs /tmp/oba-browser     # cases + TS frames + page
OBA_CANVAS_DIR=/tmp/oba-browser cargo test -p bot-avatar --test canvas_compare -- --ignored
python3 -m http.server 8765 --bind 127.0.0.1 --directory /tmp/oba-browser
# open http://127.0.0.1:8765/ and run `await compare()` in the console
```

`compare()` reports per case the pixels either side painted, the mean and the largest
difference of premultiplied channels (0..255) over them, and how many pixels differ by more
than 8, 32 and 96. Differences are expected only along antialiased edges (Skia's and
tiny-skia's coverage differ by a few levels).
