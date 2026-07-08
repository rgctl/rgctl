import type { CommunitiesPayload } from "./types";

/** Sigma v3 only parses hex and rgb()/rgba() — not hsl(). */
export function communityColorHex(index: number): string {
  const hue = (index * 47 + 210) % 360;
  return hslToHex(hue, 58, 52);
}

export function hslToHex(h: number, s: number, l: number): string {
  const sat = s / 100;
  const lit = l / 100;
  const c = (1 - Math.abs(2 * lit - 1)) * sat;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = lit - c / 2;
  let r = 0;
  let g = 0;
  let b = 0;
  if (h < 60) {
    r = c;
    g = x;
  } else if (h < 120) {
    r = x;
    g = c;
  } else if (h < 180) {
    g = c;
    b = x;
  } else if (h < 240) {
    g = x;
    b = c;
  } else if (h < 300) {
    r = x;
    b = c;
  } else {
    r = c;
    b = x;
  }
  const toByte = (v: number) => Math.round((v + m) * 255);
  return rgbToHex(toByte(r), toByte(g), toByte(b));
}

export function rgbToHex(r: number, g: number, b: number): string {
  const h = (n: number) => n.toString(16).padStart(2, "0");
  return `#${h(r)}${h(g)}${h(b)}`;
}

/** Accept legacy hsl() strings from older bundles. */
export function normalizeGraphColor(color: string): string {
  const trimmed = color.trim();
  if (trimmed.startsWith("#")) return trimmed;
  const hsl = /^hsl\(\s*(\d+)\s+([\d.]+)%\s+([\d.]+)%\s*\)$/i.exec(trimmed);
  if (hsl) {
    return hslToHex(Number(hsl[1]), Number(hsl[2]), Number(hsl[3]));
  }
  return trimmed;
}

export function buildCommunityColorMap(
  communities: CommunitiesPayload | null,
): Map<number, string> {
  const map = new Map<number, string>();
  for (const c of communities?.communities ?? []) {
    map.set(c.id, normalizeGraphColor(c.color));
  }
  return map;
}

export function resolveCommunityColor(
  communityId: number | null | undefined,
  colorMap: Map<number, string>,
): string {
  const cid = communityId ?? 0;
  return colorMap.get(cid) ?? communityColorHex(cid);
}

export function fadeColor(color: string, alpha: number): string {
  const { r, g, b } = parseRgbColor(color);
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function parseRgbColor(color: string): { r: number; g: number; b: number } {
  const hex = normalizeGraphColor(color);
  if (hex.startsWith("#")) {
    const h = hex.slice(1);
    const full =
      h.length === 3
        ? h
            .split("")
            .map((c) => c + c)
            .join("")
        : h.slice(0, 6);
    return {
      r: parseInt(full.slice(0, 2), 16),
      g: parseInt(full.slice(2, 4), 16),
      b: parseInt(full.slice(4, 6), 16),
    };
  }
  const rgba = /^rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/i.exec(color);
  if (rgba) {
    return { r: Number(rgba[1]), g: Number(rgba[2]), b: Number(rgba[3]) };
  }
  return { r: 111, g: 66, b: 193 };
}

/** Sigma built-in programs: circle (disc) and point (square). */
export type SigmaNodeProgram = "circle" | "point";

/** Distinct hex palette for member node types (Sigma-safe). */
export const NODE_TYPE_COLORS: Record<number, string> = {
  0: "#2563eb", // Function
  1: "#7c3aed", // Class
  2: "#059669", // Struct
  3: "#d97706", // Enum
  4: "#db2777", // Interface
  5: "#475569", // Module
};

export function resolveNodeTypeColor(nodeType: number): string {
  return NODE_TYPE_COLORS[nodeType] ?? "#6c757d";
}

export function nodeTypeColorForBit(bit: number): string {
  if (bit <= 0) return "#6c757d";
  const index = 31 - Math.clz32(bit);
  return resolveNodeTypeColor(index);
}

export function sigmaProgramForNodeType(nodeType: number): SigmaNodeProgram {
  switch (nodeType) {
    case 0: // Function
    case 4: // Interface
      return "circle";
    case 1: // Class
    case 2: // Struct
    case 5: // Module
      return "point";
    default:
      return "circle";
  }
}

/** Package rollup shape from member counts. */
export function sigmaProgramForMetanode(functions: number, classes: number): SigmaNodeProgram {
  if (classes > 0 && functions === 0) return "point";
  return "circle";
}

export function nodeTypeSizeScale(nodeType: number): number {
  switch (nodeType) {
    case 2:
      return 0.9;
    case 5:
      return 1.12;
    case 4:
      return 1.05;
    default:
      return 1;
  }
}

export const NODE_TYPE_LEGEND: Array<{
  label: string;
  program: SigmaNodeProgram;
  color: string;
  hint: string;
}> = [
  { label: "Function", program: "circle", color: NODE_TYPE_COLORS[0], hint: "disc" },
  { label: "Class", program: "point", color: NODE_TYPE_COLORS[1], hint: "square" },
  { label: "Struct", program: "point", color: NODE_TYPE_COLORS[2], hint: "square" },
  { label: "Interface", program: "circle", color: NODE_TYPE_COLORS[4], hint: "disc" },
  { label: "Module", program: "point", color: NODE_TYPE_COLORS[5], hint: "square" },
];
