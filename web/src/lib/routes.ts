const WS_BASE_URL = import.meta.env.VITE_WS_BASE_URL;

export const ApiRoutes = {
  ws: `${WS_BASE_URL}/ws`,
} as const;
