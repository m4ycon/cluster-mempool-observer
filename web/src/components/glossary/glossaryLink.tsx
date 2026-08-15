import { cloneElement, Fragment, isValidElement, type ReactNode } from 'react';
import { GLOSSARY, type GlossaryTerm } from '../../lib/glossary';
import { Term } from './Term';

interface Candidate {
  term: GlossaryTerm;
  regex: RegExp;
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** Sticky, case-insensitive, whole-word regex for one candidate spelling, plural-tolerant. */
function candidateRegex(phrase: string): RegExp {
  const pattern = escapeRegExp(phrase).replace(/ /g, '\\s+');
  return new RegExp(`\\b${pattern}s?\\b`, 'iy');
}

/**
 * Every spelling that can trigger a link, longest phrase first: at a given
 * start position this makes the fuller phrase (e.g. "sigops-adjusted weight")
 * win over a shorter one it contains (e.g. "weight").
 */
function buildCandidates(exclude?: GlossaryTerm): Candidate[] {
  const entries: { term: GlossaryTerm; phrase: string }[] = [];
  for (const term of Object.keys(GLOSSARY) as GlossaryTerm[]) {
    if (term === exclude) continue;
    const { label, aliases } = GLOSSARY[term];
    // Labels like "weight (WU)" carry a display-only annotation; the bare
    // word is what actually shows up in prose, and the annotation itself
    // (e.g. "WU") is covered by its own alias when one exists.
    const bareLabel = label.replace(/\s*\([^)]*\)\s*$/, '');
    entries.push({ term, phrase: bareLabel });
    for (const alias of aliases ?? []) {
      entries.push({ term, phrase: alias });
    }
  }
  entries.sort((a, b) => b.phrase.length - a.phrase.length);
  return entries.map(({ term, phrase }) => ({
    term,
    regex: candidateRegex(phrase),
  }));
}

/** Links the first not-yet-used candidate match starting at `text[from]`, if any. */
function matchAt(
  text: string,
  from: number,
  candidates: Candidate[],
  used: Set<GlossaryTerm>,
): { term: GlossaryTerm; length: number } | null {
  for (const candidate of candidates) {
    if (used.has(candidate.term)) continue;
    candidate.regex.lastIndex = from;
    const match = candidate.regex.exec(text);
    if (match) return { term: candidate.term, length: match[0].length };
  }
  return null;
}

function linkString(
  text: string,
  candidates: Candidate[],
  used: Set<GlossaryTerm>,
): ReactNode {
  let pieces: ReactNode[] | null = null;
  let flushed = 0;
  let key = 0;
  let i = 0;

  while (i < text.length) {
    const found = matchAt(text, i, candidates, used);
    if (!found) {
      i += 1;
      continue;
    }
    pieces ??= [];
    if (i > flushed) pieces.push(text.slice(flushed, i));
    const original = text.slice(i, i + found.length);
    used.add(found.term);
    pieces.push(
      <Term key={key++} term={found.term}>
        {original}
      </Term>,
    );
    i += found.length;
    flushed = i;
  }

  if (!pieces) return text;
  if (flushed < text.length) pieces.push(text.slice(flushed));
  return pieces;
}

/** Transforms a React node by linking glossary terms within it. */
function transformNode(
  node: ReactNode,
  candidates: Candidate[],
  used: Set<GlossaryTerm>,
): ReactNode {
  if (typeof node === 'string') return linkString(node, candidates, used);

  if (Array.isArray(node)) {
    return node.map((child, i) => (
      // biome-ignore lint/suspicious/noArrayIndexKey: node order here is fixed prose content
      <Fragment key={i}>{transformNode(child, candidates, used)}</Fragment>
    ));
  }

  if (isValidElement(node)) {
    // Already a glossary link (ours or hand-authored) -- leave its contents untouched.
    if (node.type === Term) return node;

    const props = node.props as { children?: ReactNode };
    if (!('children' in props)) return node;
    return cloneElement(
      node,
      undefined,
      transformNode(props.children, candidates, used),
    );
  }

  return node;
}

export interface GlossResult {
  node: ReactNode;
  /** Terms that got an inline link, in the order they were matched. */
  linked: Set<GlossaryTerm>;
}

/** Links the first mention of each glossary term found in `children` to its definition. */
export function glossLinks(
  children: ReactNode,
  exclude?: GlossaryTerm,
): GlossResult {
  const linked = new Set<GlossaryTerm>();
  const node = transformNode(children, buildCandidates(exclude), linked);
  return { node, linked };
}

export interface GlossedProps {
  children: ReactNode;
  /** Term whose own dialog this is -- never link it inside its own body. */
  exclude?: GlossaryTerm;
}

/** Thin component wrapper over {@link glossLinks} for callers that don't need the linked set. */
export function Glossed({ children, exclude }: GlossedProps): ReactNode {
  return glossLinks(children, exclude).node;
}
