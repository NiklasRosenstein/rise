import { CONFIG } from './config';

export async function login(): Promise<void> {
  const returnUrl = encodeURIComponent(window.location.href);
  window.location.href = `${CONFIG.backendUrl}/api/v1/auth/signin/start?rd=${returnUrl}`;
}

export async function logout(): Promise<void> {
  try {
    await fetch(`${CONFIG.backendUrl}/api/v1/auth/logout`, {
      method: 'GET',
      credentials: 'include'
    });
  } catch {
    // Best-effort logout.
  }
  window.location.href = '/';
}
