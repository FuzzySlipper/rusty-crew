import { constants } from "node:fs";
import { open, realpath, type FileHandle } from "node:fs/promises";
import { relative, resolve, sep } from "node:path";

function isOutside(root: string, target: string): boolean {
  const pathFromRoot = relative(root, target);
  return (
    pathFromRoot === ".." ||
    pathFromRoot.startsWith(`..${sep}`) ||
    pathFromRoot.startsWith(sep)
  );
}

function procFileDescriptorPath(handle: FileHandle, child?: string): string {
  const descriptorPath = `/proc/self/fd/${handle.fd}`;
  return child === undefined ? descriptorPath : `${descriptorPath}/${child}`;
}

function delegatedOpenError(component: string, error: unknown): Error {
  const code = (error as NodeJS.ErrnoException).code;
  if (code === "ELOOP" || code === "ENOTDIR") {
    return new Error(
      `delegated workspace path contains symlink or invalid directory: ${component}`,
    );
  }
  return error instanceof Error ? error : new Error(String(error));
}

async function closeAll(handles: readonly FileHandle[]): Promise<void> {
  await Promise.all(handles.map((handle) => handle.close().catch(() => {})));
}

/**
 * Open a delegated-workspace file relative to pinned directory descriptors.
 * Every directory component and the final file use O_NOFOLLOW, so replacing a
 * checked parent pathname cannot redirect creation or mutation outside the
 * descriptor-rooted tree.
 */
export async function openConstrainedMutableFile(
  root: string,
  target: string,
  create: boolean,
): Promise<FileHandle> {
  const resolvedRoot = resolve(root);
  const resolvedTarget = resolve(target);
  if (isOutside(resolvedRoot, resolvedTarget)) {
    throw new Error(`path escapes delegated workspace: ${target}`);
  }

  const canonicalRoot = await realpath(resolvedRoot);
  const pathFromRoot = relative(resolvedRoot, resolvedTarget);
  const components = pathFromRoot.split(sep).filter(Boolean);
  if (components.length === 0) {
    throw new Error(`delegated workspace target is not a file: ${target}`);
  }

  const directories: FileHandle[] = [];
  try {
    let directory = await open(
      resolvedRoot,
      constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW,
    );
    directories.push(directory);
    if ((await realpath(procFileDescriptorPath(directory))) !== canonicalRoot) {
      throw new Error(`delegated workspace root changed during open: ${root}`);
    }

    for (const component of components.slice(0, -1)) {
      try {
        directory = await open(
          procFileDescriptorPath(directory, component),
          constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW,
        );
      } catch (error) {
        throw delegatedOpenError(component, error);
      }
      directories.push(directory);
    }

    const leafPath = procFileDescriptorPath(
      directory,
      components[components.length - 1],
    );
    let handle: FileHandle;
    try {
      handle = await open(leafPath, constants.O_RDWR | constants.O_NOFOLLOW);
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ELOOP") {
        throw delegatedOpenError(components[components.length - 1], error);
      }
      if (!create || (error as NodeJS.ErrnoException).code !== "ENOENT") {
        throw error;
      }
      handle = await open(
        leafPath,
        constants.O_RDWR |
          constants.O_NOFOLLOW |
          constants.O_CREAT |
          constants.O_EXCL,
        0o666,
      );
    }

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

      const openedPath = await realpath(procFileDescriptorPath(handle));
      if (
        isOutside(canonicalRoot, openedPath) ||
        openedPath === canonicalRoot
      ) {
        throw new Error(`path escapes delegated workspace: ${target}`);
      }
      return handle;
    } catch (error) {
      await handle.close();
      throw error;
    }
  } finally {
    await closeAll(directories);
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
