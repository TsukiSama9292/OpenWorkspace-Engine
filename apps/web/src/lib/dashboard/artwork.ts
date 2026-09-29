export type TemplateFamily = 'ubuntu' | 'python' | 'pytorch' | 'rust' | 'jupyter' | 'generic';

const FAMILY_ORDER: TemplateFamily[] = ['pytorch', 'ubuntu', 'python', 'rust', 'jupyter'];

export function resolveTemplateFamily(name: string): TemplateFamily {
  const lower = name.toLowerCase();
  for (const family of FAMILY_ORDER) {
    if (lower.includes(family)) return family;
  }
  return 'generic';
}

export function familyIconSrc(family: TemplateFamily): string {
  return `/icons/${family}.svg`;
}

export function familyCoverClass(family: TemplateFamily): string {
  return `catalog-cover family-${family}`;
}
