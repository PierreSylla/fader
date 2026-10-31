import assert from "node:assert/strict";
import { test } from "node:test";
import {
  valueFromBottom,
  valueFromKey,
  valueFromLeft,
  valueFromWheel,
} from "./input.ts";

test("a vertical position maps bottom to 0 and top to 100", () => {
  const rect = { bottom: 200, height: 100 };

  assert.equal(valueFromBottom(rect, 200), 0);
  assert.equal(valueFromBottom(rect, 100), 100);
  assert.equal(valueFromBottom(rect, 150), 50);
  assert.equal(valueFromBottom(rect, 500), 0);
  assert.equal(valueFromBottom(rect, -500), 100);
});

test("a horizontal position maps left to 0 and right to 100", () => {
  const rect = { left: 10, width: 200 };

  assert.equal(valueFromLeft(rect, 10), 0);
  assert.equal(valueFromLeft(rect, 210), 100);
  assert.equal(valueFromLeft(rect, 110), 50);
});

test("keys follow the design", () => {
  assert.equal(valueFromKey("ArrowUp", 50), 51);
  assert.equal(valueFromKey("ArrowRight", 50), 51);
  assert.equal(valueFromKey("ArrowDown", 50), 49);
  assert.equal(valueFromKey("ArrowLeft", 50), 49);
  assert.equal(valueFromKey("PageUp", 50), 60);
  assert.equal(valueFromKey("PageDown", 50), 40);
  assert.equal(valueFromKey("Home", 50), 0);
  assert.equal(valueFromKey("End", 50), 100);
  assert.equal(valueFromKey("PageUp", 95), 100);
  assert.equal(valueFromKey("PageDown", 5), 0);
  assert.equal(valueFromKey("Tab", 50), null);
  assert.equal(valueFromKey("Escape", 50), null);
});

test("the wheel moves by 1, or by the larger step with shift", () => {
  assert.equal(valueFromWheel(-1, false, 50), 51);
  assert.equal(valueFromWheel(1, false, 50), 49);
  assert.equal(valueFromWheel(-1, true, 50), 55);
  assert.equal(valueFromWheel(1, true, 50), 45);
  assert.equal(valueFromWheel(-1, true, 98), 100);
  assert.equal(valueFromWheel(1, false, 0), 0);
});
