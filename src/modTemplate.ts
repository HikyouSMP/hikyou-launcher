export interface TemplateEntry { projectId: string; name: string; iconUrl: string | null; loaders: string[] | null; folderId: string | null }
export interface TemplateFolder { id: string; parentId: string | null; name: string }
export interface ModTemplate { revision: string; entries: TemplateEntry[]; folders: TemplateFolder[]; legacyEntries: TemplateEntry[]; migrationRequired: boolean }
export interface TemplatePreview { revision: string; token: string; items: { projectId: string; name: string; iconUrl: string | null; status: 'available' | 'unavailable'; reason: string | null }[] }
export interface TemplateApplication { revision: string; token: string; projectIds: string[] }
export function dissolveFolder(value: ModTemplate, id: string) {
  const parent = value.folders.find(folder => folder.id === id)?.parentId ?? null;
  return { entries: value.entries.map(entry => entry.folderId === id ? { ...entry, folderId: parent } : entry),
    folders: value.folders.filter(folder => folder.id !== id).map(folder => folder.parentId === id ? { ...folder, parentId: parent } : folder) };
}
export function folderLabel(folders: TemplateFolder[], id: string): string {
  const names: string[] = []; const seen = new Set<string>(); let current: string | null = id;
  while (current && !seen.has(current) && names.length < 16) { seen.add(current); const folder = folders.find(folder => folder.id === current); if (!folder) break; names.unshift(folder.name); current = folder.parentId; }
  return names.join(' / ');
}
