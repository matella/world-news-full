import { createHash, timingSafeEqual } from 'node:crypto';

export type AdminAccess = 'allowed' | 'unauthorised' | 'unconfigured';

function equal(left: string, right: string): boolean {
  return timingSafeEqual(createHash('sha256').update(left).digest(), createHash('sha256').update(right).digest());
}

// Transitional protection for the read-only dashboard; use TLS at the public proxy.
export function adminAccess(header: string | null, user?: string, password?: string): AdminAccess {
  if (!user || !password) return 'unconfigured';
  if (!header || header.length > 2048 || !/^Basic [A-Za-z0-9+/]+={0,2}$/i.test(header)) return 'unauthorised';
  const decoded = Buffer.from(header.slice(6), 'base64').toString('utf8');
  const separator = decoded.indexOf(':');
  if (separator < 0) return 'unauthorised';
  const validUser = equal(decoded.slice(0, separator), user);
  const validPassword = equal(decoded.slice(separator + 1), password);
  return validUser && validPassword ? 'allowed' : 'unauthorised';
}
