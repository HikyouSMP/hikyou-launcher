export async function readFileBytes(file: File, maxBytes: number): Promise<number[]> {
  if (file.size === 0 || file.size > maxBytes) {
    throw new Error(`File must be between 1 byte and ${Math.floor(maxBytes / 1024 / 1024)} MiB.`);
  }
  return Array.from(new Uint8Array(await file.arrayBuffer()));
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
}
