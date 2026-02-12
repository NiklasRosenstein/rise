export async function copyToClipboard(text: string): Promise<void> {
  if (navigator.clipboard && navigator.clipboard.writeText) {
    return navigator.clipboard.writeText(text);
  }

  const textarea = document.createElement('textarea');
  textarea.value = text;
  textarea.style.position = 'fixed';
  textarea.style.opacity = '0';
  document.body.appendChild(textarea);
  textarea.select();

  try {
    document.execCommand('copy');
    document.body.removeChild(textarea);
  } catch {
    document.body.removeChild(textarea);
    throw new Error('Clipboard not available');
  }
}

export function formatDate(dateString: string): string {
  const date = new Date(dateString);
  return date.toLocaleString();
}

export function formatTimeRemaining(expiresAt: string | null | undefined): string | null {
  if (!expiresAt) return null;

  const now = new Date();
  const expiryDate = new Date(expiresAt);
  const diffMs = expiryDate.getTime() - now.getTime();
  const diffSec = Math.floor(Math.abs(diffMs) / 1000);
  const diffMin = Math.floor(diffSec / 60);
  const diffHour = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHour / 24);

  const isExpired = diffMs < 0;
  const prefix = isExpired ? 'expired ' : 'in ';
  const suffix = isExpired ? ' ago' : '';

  if (diffDay > 0) return `${prefix}${diffDay} day${diffDay > 1 ? 's' : ''}${suffix}`;
  if (diffHour > 0) return `${prefix}${diffHour} hour${diffHour > 1 ? 's' : ''}${suffix}`;
  if (diffMin > 0) return `${prefix}${diffMin} minute${diffMin > 1 ? 's' : ''}${suffix}`;
  return `${prefix}${diffSec} second${diffSec !== 1 ? 's' : ''}${suffix}`;
}
