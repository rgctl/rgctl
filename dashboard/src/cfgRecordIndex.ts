const INDEX_MAGIC = [0x52, 0x42, 0x43, 0x49]; // RBCI

export interface CfgRecordLocation {
  offset: number;
  length: number;
}

function formatUuid(bytes: Uint8Array): string {
  const hex = [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export function parseCfgRecordIndex(bytes: Uint8Array): Map<string, CfgRecordLocation> {
  if (bytes.length < 16) {
    throw new Error("cfg record index truncated");
  }
  for (let i = 0; i < 4; i++) {
    if (bytes[i] !== INDEX_MAGIC[i]) {
      throw new Error("invalid cfg record index magic");
    }
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const version = view.getUint32(4, true);
  if (version !== 1) {
    throw new Error(`unsupported cfg record index version ${version}`);
  }
  const count = Number(view.getBigUint64(8, true));
  const entries = new Map<string, CfgRecordLocation>();
  let pos = 16;
  const entrySize = 16 + 8 + 4;
  if (bytes.length < pos + count * entrySize) {
    throw new Error("cfg record index payload truncated");
  }
  for (let i = 0; i < count; i++) {
    const id = formatUuid(bytes.slice(pos, pos + 16));
    pos += 16;
    const offset = Number(view.getBigUint64(pos, true));
    pos += 8;
    const length = view.getUint32(pos, true);
    pos += 4;
    entries.set(id, { offset, length });
  }
  return entries;
}

export async function fetchCfgRecordBytes(
  dataUrl: string | URL,
  location: CfgRecordLocation,
): Promise<Uint8Array> {
  const end = location.offset + location.length - 1;
  const res = await fetch(dataUrl, {
    headers: { Range: `bytes=${location.offset}-${end}` },
  });
  if (!(res.ok || res.status === 206)) {
    throw new Error(`cfg record fetch HTTP ${res.status}`);
  }
  const body = new Uint8Array(await res.arrayBuffer());
  if (res.status === 206 && body.length === location.length) {
    return body;
  }
  if (body.length < location.offset + location.length) {
    throw new Error("cfg record fetch truncated");
  }
  return body.slice(location.offset, location.offset + location.length);
}
