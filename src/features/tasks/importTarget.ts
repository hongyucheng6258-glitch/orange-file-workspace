export function resolveImportParentId(
  pathname: string,
  currentParentId: string | null,
): string | null {
  return pathname === "/files" ? currentParentId : null;
}
