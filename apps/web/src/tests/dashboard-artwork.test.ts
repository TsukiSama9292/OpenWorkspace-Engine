import { describe, it, expect } from 'vitest';
import { resolveTemplateFamily, familyIconSrc, familyCoverClass, familyArtwork, type TemplateFamily } from '$lib/dashboard/artwork';

describe('resolveTemplateFamily', () => {
  it.each<[string, TemplateFamily]>([
    ['Ubuntu 22.04', 'ubuntu'],
    ['UBUNTU-jammy', 'ubuntu'],
    ['python 3.12', 'python'],
    ['PyTorch cuda', 'pytorch'],
    ['rust nightly', 'rust'],
    ['Jupyter lab', 'jupyter']
  ])('resolves %s to %s', (name, family) => {
    expect(resolveTemplateFamily(name)).toBe(family);
  });

  it('falls back to generic for unknown names', () => {
    expect(resolveTemplateFamily('my-custom-thing')).toBe('generic');
    expect(resolveTemplateFamily('')).toBe('generic');
  });

  it('prefers pytorch over python when both match', () => {
    expect(resolveTemplateFamily('pytorch-python')).toBe('pytorch');
  });
});

describe('familyIconSrc', () => {
  it('serves every family from the local static tier', () => {
    const families: TemplateFamily[] = ['ubuntu', 'python', 'pytorch', 'rust', 'jupyter', 'generic'];
    for (const family of families) {
      expect(familyIconSrc(family)).toBe(`/icons/${family}.svg`);
    }
  });

  it('maps every family to its cover class', () => {
    expect(familyCoverClass('ubuntu')).toBe('catalog-cover family-ubuntu');
    expect(familyCoverClass('generic')).toBe('catalog-cover family-generic');
  });

  it('bundles the family, mark, and cover behind one call', () => {
    expect(familyArtwork('Ubuntu box')).toEqual({
      family: 'ubuntu',
      iconSrc: '/icons/ubuntu.svg',
      coverClass: 'catalog-cover family-ubuntu'
    });
    expect(familyArtwork('mystery')).toEqual({
      family: 'generic',
      iconSrc: '/icons/generic.svg',
      coverClass: 'catalog-cover family-generic'
    });
  });
});
