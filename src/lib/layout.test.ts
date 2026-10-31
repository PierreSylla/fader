import assert from "node:assert/strict";
import { test } from "node:test";
import { computeLayout } from "./layout.ts";

const landscape = { width: 2560, height: 1440 };

test("an empty list gives an empty layout", () => {
  assert.deepEqual(computeLayout([]), { width: 0, height: 0, screens: [] });
});

test("stays within the design limits", () => {
  const layout = computeLayout(
    ["a", "b", "c"].map((id) => ({ id, ...landscape })),
  );

  assert.ok(layout.width <= 420, `width ${layout.width}`);
  assert.ok(layout.height <= 190, `height ${layout.height}`);
});

test("keeps the order, the gap and the bottom alignment", () => {
  const { screens, height } = computeLayout([
    { id: "small", width: 1504, height: 1003 },
    { id: "big", ...landscape },
  ]);

  assert.deepEqual(
    screens.map((s) => s.id),
    ["small", "big"],
  );
  assert.equal(screens[1].x, screens[0].x + screens[0].width + 8);
  for (const screen of screens) {
    assert.equal(screen.y + screen.height, height);
  }
});

test("softens proportions but keeps a bigger monitor bigger", () => {
  const { screens } = computeLayout([
    { id: "small", width: 1504, height: 1003 },
    { id: "big", ...landscape },
  ]);
  const areaRatio =
    (screens[1].width * screens[1].height) /
    (screens[0].width * screens[0].height);

  assert.ok(areaRatio > 1);
  assert.ok(areaRatio < (2560 * 1440) / (1504 * 1003));
});

test("a portrait monitor stays taller than wide", () => {
  const { screens } = computeLayout([
    { id: "portrait", width: 1080, height: 1920 },
    { id: "wide", ...landscape },
  ]);

  assert.ok(screens[0].height > screens[0].width);
});

test("respects the minimum size of a tile", () => {
  const { screens } = computeLayout([
    { id: "tiny", width: 640, height: 480 },
    { id: "huge", width: 7680, height: 4320 },
  ]);

  for (const screen of screens) {
    assert.ok(screen.width >= 72, `width ${screen.width}`);
    assert.ok(screen.height >= 56, `height ${screen.height}`);
  }
});
