export function displayPathName(path: string): string {
  const normalized = path.replace(/[\\/]+$/u, "");
  const name = normalized.replace(/^.*[\\/]/u, "");
  return name.length > 0 ? name : path;
}
