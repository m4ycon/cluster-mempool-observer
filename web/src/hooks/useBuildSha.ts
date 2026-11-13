import { useEffect, useState } from 'react';
import { HttpClient } from '../lib/http';

const webClient = new HttpClient('');

/** Short commit the web container was stamped with; undefined until known or when unstamped. */
export function useBuildSha(): string | undefined {
  const [sha, setSha] = useState<string>();

  useEffect(() => {
    webClient
      .get<{ sha: string }>('/version.json')
      .then((v) => setSha(v.sha.slice(0, 7) || undefined))
      .catch(() => {});
  }, []);

  return sha;
}
