import type { ReactNode } from 'react';
import { GLOSSARY, type GlossaryTerm } from '../../lib/glossary';
import { Dialog } from '../dialog/Dialog';
import { Glossed } from './glossaryLink';

export interface TermProps {
  term: GlossaryTerm;
  children?: ReactNode;
}

/** Opens a term's definition; shared with triggers that cannot render a `<Term>`. */
export function openTermDialog(term: GlossaryTerm) {
  const entry = GLOSSARY[term];

  Dialog.call({
    title: entry.label,
    body: <Glossed exclude={term}>{entry.text}</Glossed>,
  });
}

/** Inline jargon trigger: opens the glossary entry for `term` in the shared dialog. */
export function Term({ term, children }: TermProps) {
  const entry = GLOSSARY[term];

  return (
    <button
      type="button"
      onClick={() => openTermDialog(term)}
      aria-label={`Definition: ${entry.label}`}
      className="mco-reset inline underline decoration-dotted underline-offset-2 hover:text-orange focus-visible:text-orange"
    >
      {children ?? entry.label}
    </button>
  );
}
