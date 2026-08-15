import clsx from 'clsx';
import { Dialog } from '../dialog/Dialog';
import { glossLinks } from '../glossary/glossaryLink';
import { Term } from '../glossary/Term';
import { type HelpTopic, PANEL_HELP } from './panelHelp';

export interface HelpButtonProps {
  topic: HelpTopic;
}

/** Small "?" trigger next to a panel title or control label; opens its help entry. */
export function HelpButton({ topic }: HelpButtonProps) {
  const entry = PANEL_HELP[topic];

  const handleClick = () => {
    const { node, linked } = glossLinks(entry.body);
    // Only list terms the prose didn't already link -- keeps the two in sync automatically.
    const seeAlso = entry.terms.filter((term) => !linked.has(term));

    Dialog.call({
      title: entry.title,
      body: node,
      footer: seeAlso.length > 0 && (
        <div className="flex flex-wrap items-center gap-2 text-xs">
          <span className="text-dim">See also</span>
          {seeAlso.map((term) => (
            <Term key={term} term={term} />
          ))}
        </div>
      ),
    });
  };

  return (
    <button
      type="button"
      onClick={handleClick}
      aria-label={`Help: ${entry.title}`}
      className={clsx(
        'mco-reset inline-flex size-4.5 items-center justify-center rounded-full leading-none',
        'border border-bar text-[11px] text-bar',
        'hover:border-orange hover:bg-orange hover:text-bg',
        'focus-visible:border-orange focus-visible:bg-orange focus-visible:text-bg',
      )}
    >
      ?
    </button>
  );
}
