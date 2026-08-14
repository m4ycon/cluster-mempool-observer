import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { TablePagination } from './TablePagination';

/** Owns `page` so a committed change actually round-trips back into the input. */
function ControlledPagination({ pageCount }: { pageCount: number }) {
  const [page, setPage] = useState(1);
  return (
    <TablePagination page={page} pageCount={pageCount} onPageChange={setPage} />
  );
}

describe('TablePagination arrows', () => {
  it('disables prev on page 1 and next on the last page, and steps between them', () => {
    render(<ControlledPagination pageCount={3} />);

    expect(screen.getByLabelText('Previous page')).toBeDisabled();
    expect(screen.getByLabelText('Next page')).not.toBeDisabled();

    fireEvent.click(screen.getByLabelText('Next page'));
    fireEvent.click(screen.getByLabelText('Next page'));
    expect(screen.getByLabelText('Jump to page')).toHaveValue('3');
    expect(screen.getByLabelText('Next page')).toBeDisabled();

    fireEvent.click(screen.getByLabelText('Previous page'));
    expect(screen.getByLabelText('Jump to page')).toHaveValue('2');
  });
});

describe('TablePagination jump input', () => {
  it('commits a valid in-range page on blur', () => {
    render(<ControlledPagination pageCount={5} />);
    const input = screen.getByLabelText('Jump to page');

    fireEvent.change(input, { target: { value: '4' } });
    fireEvent.blur(input);

    expect(input).toHaveValue('4');
  });

  it('commits on Enter', () => {
    render(<ControlledPagination pageCount={5} />);
    const input = screen.getByLabelText('Jump to page');

    fireEvent.change(input, { target: { value: '3' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    expect(input).toHaveValue('3');
  });

  it('snaps a junk entry back to the current page without reporting it', () => {
    const onPageChange = vi.fn();
    render(
      <TablePagination page={2} pageCount={5} onPageChange={onPageChange} />,
    );
    const input = screen.getByLabelText('Jump to page');

    fireEvent.change(input, { target: { value: 'abc' } });
    fireEvent.blur(input);

    expect(input).toHaveValue('2');
    expect(onPageChange).not.toHaveBeenCalled();
  });

  it('snaps an out-of-range entry back to the current page without reporting it', () => {
    const onPageChange = vi.fn();
    render(
      <TablePagination page={2} pageCount={5} onPageChange={onPageChange} />,
    );
    const input = screen.getByLabelText('Jump to page');

    fireEvent.change(input, { target: { value: '99' } });
    fireEvent.blur(input);

    expect(input).toHaveValue('2');
    expect(onPageChange).not.toHaveBeenCalled();
  });

  it('allows the field to go empty mid-edit without reporting a page', () => {
    const onPageChange = vi.fn();
    render(
      <TablePagination page={2} pageCount={5} onPageChange={onPageChange} />,
    );
    const input = screen.getByLabelText('Jump to page');

    fireEvent.change(input, { target: { value: '' } });
    expect(input).toHaveValue('');
    expect(onPageChange).not.toHaveBeenCalled();
  });
});

describe('TablePagination page text', () => {
  it('shows PAGE n OF m', () => {
    render(<ControlledPagination pageCount={7} />);

    expect(screen.getByText(/PAGE/)).toBeInTheDocument();
    expect(screen.getByText(/OF 7/)).toBeInTheDocument();
  });
});
