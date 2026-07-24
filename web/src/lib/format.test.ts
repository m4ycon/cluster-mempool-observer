import { describe, expect, it } from 'vitest';
import { NumberFormat } from './format';

describe('NumberFormat.grouped', () => {
  it('adds thousands separators', () => {
    expect(NumberFormat.grouped(1000)).toBe('1,000');
    expect(NumberFormat.grouped(1234567)).toBe('1,234,567');
    expect(NumberFormat.grouped(42)).toBe('42');
  });
});

describe('NumberFormat.compact', () => {
  it('rounds values under 1000 with no suffix', () => {
    expect(NumberFormat.compact(0)).toBe('0');
    expect(NumberFormat.compact(42.6)).toBe('43');
    expect(NumberFormat.compact(999)).toBe('999');
  });

  it('uses a compact thousands/millions suffix at 1000+', () => {
    expect(NumberFormat.compact(1000)).toBe('1K');
    expect(NumberFormat.compact(1200)).toBe('1.2K');
    expect(NumberFormat.compact(15000)).toBe('15K');
    expect(NumberFormat.compact(3_400_000)).toBe('3.4M');
  });
});
