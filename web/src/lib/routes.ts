const WS_BASE_URL = import.meta.env.VITE_WS_BASE_URL;

export const ApiRoutes = {
  clustersDelta: `${WS_BASE_URL}/clusters/delta`,
} as const;
