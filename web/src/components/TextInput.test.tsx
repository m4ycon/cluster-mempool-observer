import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { TextInput } from './TextInput';

describe('TextInput', () => {
  it('reports every keystroke and is found by its aria-label', () => {
    const onChange = vi.fn();
    render(<TextInput value="" onChange={onChange} ariaLabel="Search" />);

    fireEvent.change(screen.getByRole('textbox', { name: 'Search' }), {
      target: { value: 'abc' },
    });
    expect(onChange).toHaveBeenCalledWith('abc');
  });

  it('offers the clear button only with `onClear` and a non-empty value', () => {
    const onClear = vi.fn();
    const { rerender } = render(
      <TextInput value="" onChange={vi.fn()} ariaLabel="Search" />,
    );
    expect(screen.queryByLabelText('Clear search')).toBeNull();

    rerender(<TextInput value="tx" onChange={vi.fn()} ariaLabel="Search" />);
    expect(screen.queryByLabelText('Clear search')).toBeNull();

    rerender(
      <TextInput
        value="tx"
        onChange={vi.fn()}
        onClear={onClear}
        ariaLabel="Search"
      />,
    );
    fireEvent.click(screen.getByLabelText('Clear search'));
    expect(onClear).toHaveBeenCalled();
  });

  it('forwards blur and key handling, which is how callers commit a value', () => {
    const onBlur = vi.fn();
    const onKeyDown = vi.fn();
    render(
      <TextInput
        value="3"
        onChange={vi.fn()}
        ariaLabel="Jump to page"
        onBlur={onBlur}
        onKeyDown={onKeyDown}
      />,
    );

    const input = screen.getByRole('textbox', { name: 'Jump to page' });
    fireEvent.keyDown(input, { key: 'Enter' });
    fireEvent.blur(input);
    expect(onKeyDown).toHaveBeenCalled();
    expect(onBlur).toHaveBeenCalled();
  });
});
