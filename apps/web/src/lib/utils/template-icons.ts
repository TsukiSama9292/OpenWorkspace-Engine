const templateIcons: Record<string, string> = {
  pytorch: '\u{1F9E0}',
  ubuntu: '\u{1F427}',
  rust: '\u2699\uFE0F',
  python: '\u{1F40D}',
  default: '\u{1F4E6}',
};

export function getTemplateIcon(name: string): string {
  const lower = name.toLowerCase();
  for (const [key, icon] of Object.entries(templateIcons)) {
    if (key !== 'default' && lower.includes(key)) return icon;
  }
  return templateIcons.default;
}
