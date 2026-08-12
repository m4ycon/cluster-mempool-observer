const MINUTE = 60;
const HOUR = 3600;
const DAY = 86400;

/** Human label for a server-chosen bucket size, e.g. `resolutionLabel(60) -> "1-minute samples"`. */
export function resolutionLabel(resolutionSecs: number): string {
  if (resolutionSecs < MINUTE) return `${resolutionSecs}-second samples`;

  if (resolutionSecs < HOUR)
    return `${Math.round(resolutionSecs / MINUTE)}-minute samples`;

  if (resolutionSecs < DAY)
    return `${Math.round(resolutionSecs / HOUR)}-hour samples`;

  return `${Math.round(resolutionSecs / DAY)}-day samples`;
}
