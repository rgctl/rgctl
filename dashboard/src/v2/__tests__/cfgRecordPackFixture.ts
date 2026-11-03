/** Minimal RBCI record-index encoder for archive CFG unit tests. */

function uuidToBytes(id: string): Uint8Array {
  const hex = id.replace(/-/g, "");
  const out = new Uint8Array(16);
  for (let i = 0; i < 16; i++) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

export function encodeRecordIndex(
  entries: { id: string; offset: number; length: number }[],
): Uint8Array {
  const buf = new ArrayBuffer(16 + entries.length * (16 + 8 + 4));
  const bytes = new Uint8Array(buf);
  const view = new DataView(buf);
  // magic RBCI
  bytes[0] = 0x52;
  bytes[1] = 0x42;
  bytes[2] = 0x43;
  bytes[3] = 0x49;
  view.setUint32(4, 1, true);
  view.setBigUint64(8, BigInt(entries.length), true);
  let pos = 16;
  for (const e of entries) {
    bytes.set(uuidToBytes(e.id), pos);
    pos += 16;
    view.setBigUint64(pos, BigInt(e.offset), true);
    pos += 8;
    view.setUint32(pos, e.length, true);
    pos += 4;
  }
  return bytes;
}
