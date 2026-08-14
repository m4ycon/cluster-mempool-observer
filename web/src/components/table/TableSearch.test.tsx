import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { TableSearch } from './TableSearch';

afterEach(() => {
  vi.useRealTimers();
});

describe('TableSearch debounce', () => {
  it('updates the input immediately but only calls onChange after 150ms', () => {
    const onChange = vi.fn();
    vi.useFakeTimers();
    render(<TableSearch value="" onChange={onChange} />);

    const input = screen.getByRole('textbox');
    fireEvent.change(input, { target: { value: 'mempool' } });

    expect(input).toHaveValue('mempool');
    expect(onChange).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(150));
    expect(onChange).toHaveBeenCalledWith('mempool');
  });

  it('debounces to only the final value of rapid typing', () => {
    const onChange = vi.fn();
    vi.useFakeTimers();
    render(<TableSearch value="" onChange={onChange} />);

    const input = screen.getByRole('textbox');
    fireEvent.change(input, { target: { value: 'm' } });
    fireEvent.change(input, { target: { value: 'me' } });
    fireEvent.change(input, { target: { value: 'mem' } });

    act(() => vi.advanceTimersByTime(150));
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenCalledWith('mem');
  });
});

describe('TableSearch external value', () => {
  it('follows a value change from outside (e.g. the URL clearing the filter)', () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <TableSearch value="stale" onChange={onChange} />,
    );
    expect(screen.getByRole('textbox')).toHaveValue('stale');

    rerender(<TableSearch value="" onChange={onChange} />);
    expect(screen.getByRole('textbox')).toHaveValue('');
  });
});

describe('TableSearch clear affordance', () => {
  it('shows a clear button only when non-empty, and clears instantly on click', () => {
    const onChange = vi.fn();
    render(<TableSearch value="" onChange={onChange} />);

    expect(screen.queryByLabelText('Clear search')).not.toBeInTheDocument();

    const input = screen.getByRole('textbox');
    fireEvent.change(input, { target: { value: 'abc' } });

    const clearButton = screen.getByLabelText('Clear search');
    fireEvent.click(clearButton);

    expect(input).toHaveValue('');
    expect(onChange).toHaveBeenCalledWith('');
  });
});
