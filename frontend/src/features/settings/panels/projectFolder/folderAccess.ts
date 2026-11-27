import { folderPaths, type FolderFiles } from '../../../../shared/api/folder';

// The browser's own folder access. Reading a picked folder works everywhere (`webkitdirectory`);
// writing one needs the File System Access API, which only some browsers have.

interface WritableFile {
  write(text: string): Promise<void>;
  close(): Promise<void>;
}
interface FileHandle {
  createWritable(): Promise<WritableFile>;
}
export interface DirectoryHandle {
  readonly name: string;
  getDirectoryHandle(name: string, options?: { create?: boolean }): Promise<DirectoryHandle>;
  getFileHandle(name: string, options?: { create?: boolean }): Promise<FileHandle>;
  removeEntry(name: string): Promise<void>;
  keys(): AsyncIterable<string>;
}
type PickDirectory = (options: { mode: 'readwrite' }) => Promise<DirectoryHandle>;

/** The browser's folder picker for writing, or nothing when it has none. */
export function directoryPicker(): PickDirectory | undefined {
  const pick = (window as unknown as { showDirectoryPicker?: PickDirectory }).showDirectoryPicker;
  return typeof pick === 'function' ? pick.bind(window) : undefined;
}

/** The project folder's files from a folder the person picked, by their path inside `.tack/`. */
export async function readPickedFolder(files: ArrayLike<File>): Promise<FolderFiles> {
  const list = Array.from(files);
  const paths = folderPaths(list.map((f) => f.webkitRelativePath || f.name));
  const read: FolderFiles = {};
  for (const file of list) {
    const path = paths.get(file.webkitRelativePath || file.name);
    if (path) read[path] = await file.text();
  }
  return read;
}

/**
 * Writes the folder the board answered into the picked one: into it when it is `.tack/`, into a
 * `.tack/` inside it otherwise. An item file the answer lacks is removed, as an export says.
 * Answers how many files were written and removed.
 */
export async function writeFolder(picked: DirectoryHandle, files: FolderFiles): Promise<{ written: number; removed: number }> {
  const root = picked.name === '.tack' ? picked : await picked.getDirectoryHandle('.tack', { create: true });
  let written = 0;
  for (const [path, text] of Object.entries(files)) {
    const parts = path.split('/');
    let dir = root;
    for (const part of parts.slice(0, -1)) dir = await dir.getDirectoryHandle(part, { create: true });
    const file = await dir.getFileHandle(parts[parts.length - 1], { create: true });
    const out = await file.createWritable();
    await out.write(text);
    await out.close();
    written += 1;
  }
  let removed = 0;
  const items = await root.getDirectoryHandle('items', { create: true });
  for await (const name of items.keys()) {
    if (name.endsWith('.md') && !(`items/${name}` in files)) {
      await items.removeEntry(name);
      removed += 1;
    }
  }
  return { written, removed };
}
