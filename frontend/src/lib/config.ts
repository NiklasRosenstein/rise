export type RiseConfig = {
  backendUrl: string;
  issuerUrl: string;
  authorizeUrl?: string;
  clientId: string;
  redirectUri: string;
  productionIngressUrlTemplate?: string;
  stagingIngressUrlTemplate?: string;
};

declare global {
  interface Window {
    CONFIG?: RiseConfig;
  }
}

export const CONFIG: RiseConfig = window.CONFIG ?? {
  backendUrl: window.location.origin,
  issuerUrl: 'http://localhost:5556/dex',
  clientId: 'rise-backend',
  redirectUri: `${window.location.origin}/`
};
