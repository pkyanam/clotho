/** Names that look like stage/test seed data — hidden on dashboard by default. */
export function isSeedRepoName(name: string): boolean {
  const n = name.toLowerCase();
  return (
    /^stage/.test(n) ||
    /^demo-stage/.test(n) ||
    /test-\d{10,}/.test(n) ||
    /-stage-test/.test(n) ||
    /stage-\d{10,}/.test(n)
  );
}

export function isSeedOrgName(name: string): boolean {
  const n = name.toLowerCase();
  return n.includes("stage") || /test-\d{10,}/.test(n);
}

export function filterSeedRepos<T extends { name: string }>(
  items: T[],
  showAll: boolean,
): T[] {
  if (showAll) return items;
  return items.filter((r) => !isSeedRepoName(r.name));
}

export function filterSeedOrgs<T extends { name: string }>(
  items: T[],
  showAll: boolean,
): T[] {
  if (showAll || items.length <= 8) return items;
  return items.filter((o) => !isSeedOrgName(o.name));
}
