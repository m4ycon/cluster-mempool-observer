import { X } from 'lucide-react';
import { type ReactNode, useEffect, useId, useRef } from 'react';
import { createCallable } from 'react-call';

export interface DialogProps {
  title: string;
  body: ReactNode;
  footer?: ReactNode;
}

/** Generic modal primitive: any panel/control opens it via `Dialog.call({...})`. */
export const Dialog = createCallable<DialogProps, void>(
  ({ call, title, body, footer }) => {
    const ref = useRef<HTMLDialogElement>(null);
    const titleId = useId();

    // <dialog> has no declarative "open as modal" prop, so this must run post-mount.
    useEffect(() => {
      ref.current?.showModal();
    }, []);

    return (
      // biome-ignore lint/a11y/useKeyWithClickEvents: backdrop-click-to-close is a mouse-only convenience; Esc already covers keyboard
      <dialog
        ref={ref}
        aria-labelledby={titleId}
        onClose={() => call.end()}
        onClick={(e) => {
          // A click that lands on the <dialog> box itself (not the inner wrapper)
          // is the standard way to detect a native-dialog backdrop click.
          if (e.target === ref.current) ref.current?.close();
        }}
        className="m-auto w-full max-w-md border border-line bg-bg p-0 text-body backdrop:bg-bg/80"
      >
        <div className="flex items-center justify-between border-line border-b px-4 py-3">
          <h2
            id={titleId}
            className="text-xs text-ink tracking-widest uppercase"
          >
            {title}
          </h2>
          <button
            type="button"
            aria-label="Close dialog"
            onClick={() => ref.current?.close()}
            className="mco-reset text-dim hover:text-orange"
          >
            <X size={16} />
          </button>
        </div>

        <div className="px-4 py-4 text-sm">{body}</div>

        {footer && (
          <div className="border-line border-t px-4 py-3">{footer}</div>
        )}
      </dialog>
    );
  },
);
