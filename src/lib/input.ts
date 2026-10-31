export const SHIFT_WHEEL_STEP = 5;

export const clampValue = (value: number): number =>
  Math.max(0, Math.min(100, Math.round(value)));

export const valueFromBottom = (
  rect: { bottom: number; height: number },
  y: number,
): number => clampValue(((rect.bottom - y) / rect.height) * 100);

export const valueFromLeft = (
  rect: { left: number; width: number },
  x: number,
): number => clampValue(((x - rect.left) / rect.width) * 100);

const KEY_STEPS: Record<string, number> = {
  ArrowUp: 1,
  ArrowRight: 1,
  ArrowDown: -1,
  ArrowLeft: -1,
  PageUp: 10,
  PageDown: -10,
};

export function valueFromKey(key: string, current: number): number | null {
  if (key === "Home") return 0;
  if (key === "End") return 100;
  const step = KEY_STEPS[key];
  return step === undefined ? null : clampValue(current + step);
}

export function valueFromWheel(
  deltaY: number,
  shift: boolean,
  current: number,
): number {
  const step = shift ? SHIFT_WHEEL_STEP : 1;
  return clampValue(current + (deltaY < 0 ? step : -step));
}
