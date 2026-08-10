import { describe, it, expect } from 'vitest';
import {
  DISABLED,
  UNLIMITED,
  isTriStateValid,
  parsePositiveInt,
  triStateFromValue,
  valueFromTriState,
} from '$lib/tri-state';

describe('triStateFromValue', () => {
  it('maps -1 to unlimited', () => {
    expect(triStateFromValue(-1)).toEqual(UNLIMITED);
  });

  it('maps 0 to disabled', () => {
    expect(triStateFromValue(0)).toEqual(DISABLED);
  });

  it('maps a positive value to custom', () => {
    expect(triStateFromValue(4)).toEqual({ mode: 'custom', value: 4 });
  });

  it('maps null/undefined to the fallback', () => {
    expect(triStateFromValue(undefined)).toEqual(UNLIMITED);
    expect(triStateFromValue(null, 0)).toEqual(DISABLED);
  });
});

describe('valueFromTriState', () => {
  it('round-trips unlimited/disabled/custom to -1/0/value', () => {
    expect(valueFromTriState(UNLIMITED)).toBe(-1);
    expect(valueFromTriState(DISABLED)).toBe(0);
    expect(valueFromTriState({ mode: 'custom', value: 8 })).toBe(8);
  });
});

describe('parsePositiveInt', () => {
  it('parses a positive integer', () => {
    expect(parsePositiveInt('4')).toBe(4);
  });

  it('rejects empty, zero, negative, and non-numeric input', () => {
    expect(parsePositiveInt('')).toBeNull();
    expect(parsePositiveInt('0')).toBeNull();
    expect(parsePositiveInt('-3')).toBeNull();
    expect(parsePositiveInt('abc')).toBeNull();
    expect(parsePositiveInt('2.5')).toBeNull();
  });
});

describe('isTriStateValid', () => {
  it('accepts unlimited, disabled, and positive custom caps', () => {
    expect(isTriStateValid(UNLIMITED)).toBe(true);
    expect(isTriStateValid(DISABLED)).toBe(true);
    expect(isTriStateValid({ mode: 'custom', value: 2 })).toBe(true);
  });

  it('rejects a custom cap that is not positive', () => {
    expect(isTriStateValid({ mode: 'custom', value: 0 })).toBe(false);
    expect(isTriStateValid({ mode: 'custom', value: -1 })).toBe(false);
  });
});
