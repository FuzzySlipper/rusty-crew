import { constants } from "node:fs";
import { lstat, open, realpath, type FileHandle } from "node:fs/promises";
import { relative, resolve, sep } from "node:path";

function isOutside(root: string, target: string): boolean {
  const pathFromRoot = relative(root, target);
  return (
    pathFromRoot === ".." ||
    pathFromRoot.startsWith(`..${sep}`) ||
    pathFromRoot.startsWith(sep)
  );
}

async function rejectSymlinkComponents(
  root: string,
  target: string,
): Promise<void> {
  const pathFromRoot = relative(root, target);
  let current = root;
  for (const component of pathFromRoot.split(sep).filter(Boolean)) {
    current = resolve(current, component);
    const metadata = await lstat(current).catch(
      (error: NodeJS.ErrnoException) => {
        if (error.code === "ENOENT") return undefined;
        throw error;
      },
    );
    if (metadata === undefined) return;
    if (metadata.isSymbolicLink()) {
      throw new Error(`delegated workspace path contains symlink: ${current}`);
    }
  }
}

/**
 * Open a delegated-workspace file without following its final component, then
 * verify the opened filesystem object is still rooted beneath the canonical
 * constraint. Callers mutate through the returned descriptor rather than
 * resolving the pathname a second time.
 */
export async function openConstrainedMutableFile(
  root: string,
  target: string,
  create: boolean,
): Promise<FileHandle> {
  const canonicalRoot = await realpath(root);
  if (isOutside(resolve(root), resolve(target))) {
    throw new Error(`path escapes delegated workspace: ${target}`);
  }
  await rejectSymlinkComponents(resolve(root), resolve(target));

  const targetExists = await lstat(target)
    .then(() => true)
    .catch((error: NodeJS.ErrnoException) => {
      if (error.code === "ENOENT") return false;
      throw error;
    });
  if (!targetExists && !create) {
    throw new Error(`delegated workspace target does not exist: ${target}`);
  }

  const flags =
    constants.O_RDWR |
    constants.O_NOFOLLOW |
    (!targetExists && create ? constants.O_CREAT | constants.O_EXCL : 0);
  const handle = await open(target, flags, 0o666);
  try {
    const metadata = await handle.stat();
    if (!metadata.isFile()) {
      throw new Error(
        `delegated workspace target is not a regular file: ${target}`,
      );
    }
    if (metadata.nlink !== 1) {
      throw new Error(
        `delegated workspace target has multiple hard links: ${target}`,
      );
    }

    const openedPath = await realpath(`/proc/self/fd/${handle.fd}`);
    if (isOutside(canonicalRoot, openedPath) || openedPath === canonicalRoot) {
      throw new Error(`path escapes delegated workspace: ${target}`);
    }

    // Catch a parent component replacement between the first walk and open.
    await rejectSymlinkComponents(resolve(root), resolve(target));
    return handle;
  } catch (error) {
    await handle.close();
    throw error;
  }
}

export async function replaceOpenFile(
  handle: FileHandle,
  content: string,
): Promise<void> {
  const bytes = Buffer.from(content, "utf8");
  await handle.truncate(0);
  let written = 0;
  while (written < bytes.length) {
    const result = await handle.write(
      bytes,
      written,
      bytes.length - written,
      written,
    );
    if (result.bytesWritten === 0) {
      throw new Error("delegated workspace write made no progress");
    }
    written += result.bytesWritten;
  }
  await handle.sync();
}
