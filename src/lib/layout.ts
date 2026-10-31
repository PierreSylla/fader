export interface Size {
  id: string;
  width: number;
  height: number;
}

export interface Placed {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Layout {
  width: number;
  height: number;
  screens: Placed[];
}

const MAX_WIDTH = 420;
const MAX_HEIGHT = 190;
const MIN_WIDTH = 72;
const MIN_HEIGHT = 56;
const GAP = 8;

// Soft proportions from the validated design: diagonal^-0.4, then a common factor.
export function computeLayout(sizes: Size[]): Layout {
  if (sizes.length === 0) return { width: 0, height: 0, screens: [] };

  const softened = sizes.map((size) => {
    const factor = Math.hypot(size.width, size.height) ** -0.4;
    return {
      id: size.id,
      width: size.width * factor,
      height: size.height * factor,
    };
  });
  const totalWidth = softened.reduce((sum, s) => sum + s.width, 0);
  const tallest = Math.max(...softened.map((s) => s.height));
  const scale = Math.min(
    (MAX_WIDTH - GAP * (sizes.length - 1)) / totalWidth,
    MAX_HEIGHT / tallest,
  );

  const scaled = softened.map((s) => {
    const width = s.width * scale;
    const height = s.height * scale;
    const grow = Math.max(1, MIN_WIDTH / width, MIN_HEIGHT / height);
    return {
      id: s.id,
      width: Math.floor(width * grow),
      height: Math.floor(height * grow),
    };
  });

  const height = Math.max(...scaled.map((s) => s.height));
  let x = 0;
  const screens = scaled.map((s) => {
    const placed = { ...s, x, y: height - s.height };
    x += s.width + GAP;
    return placed;
  });
  return { width: x - GAP, height, screens };
}
