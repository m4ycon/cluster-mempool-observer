import clsx from 'clsx';
import { ReadyState } from 'react-use-websocket';
import { Tooltip, type TooltipProps } from './Tooltip';

export interface ConnectionDotProps {
  readyState: ReadyState;
  /** Which feed the dot speaks for, e.g. `STATS FEED`. */
  label: string;
  align?: TooltipProps['align'];
}

interface Phase {
  text: 'LIVE' | 'RECONNECTING' | 'OFFLINE';
  color: 'bg-live' | 'bg-slate' | 'bg-alert';
  core: 'animate-blink-fast' | null;
  ping: 'animate-ping-live' | 'animate-ping-busy' | null;
}

/**
 * CLOSING joins CONNECTING rather than OFFLINE: both sockets reconnect
 * unconditionally, so a closing one is already on its way back.
 */
function phaseFor(readyState: ReadyState): Phase {
  switch (readyState) {
    case ReadyState.OPEN:
      return {
        text: 'LIVE',
        color: 'bg-live',
        core: null,
        ping: 'animate-ping-live',
      };
    case ReadyState.CONNECTING:
    case ReadyState.CLOSING:
      return {
        text: 'RECONNECTING',
        color: 'bg-slate',
        core: 'animate-blink-fast',
        ping: 'animate-ping-busy',
      };
    default:
      return { text: 'OFFLINE', color: 'bg-alert', core: null, ping: null };
  }
}

export function ConnectionDot({
  readyState,
  label,
  align,
}: ConnectionDotProps) {
  const phase = phaseFor(readyState);
  const description = `${label} · ${phase.text}`;

  return (
    <Tooltip label={description} align={align}>
      <span className="relative inline-flex size-2 self-center">
        {phase.ping && (
          <span
            data-testid="connection-halo"
            aria-hidden="true"
            className={clsx(
              'absolute inset-0 rounded-full',
              phase.color,
              phase.ping,
            )}
          />
        )}
        <span
          role="status"
          aria-label={description}
          className={clsx(
            'relative size-2 rounded-full',
            phase.color,
            phase.core,
          )}
        />
      </span>
    </Tooltip>
  );
}
