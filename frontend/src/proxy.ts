import { NextResponse, type NextRequest } from 'next/server';
import { adminAccess } from './lib/access';

export function proxy(request: NextRequest) {
  const access = adminAccess(request.headers.get('authorization'), process.env.NEWS_ADMIN_USER, process.env.NEWS_ADMIN_PASSWORD);
  if (access === 'allowed') return NextResponse.next();
  return new NextResponse(access === 'unconfigured' ? 'Administration indisponible.' : 'Authentification requise.', {
    status: access === 'unconfigured' ? 503 : 401,
    headers: {
      'Cache-Control': 'private, no-store',
      ...(access === 'unauthorised' ? { 'WWW-Authenticate': 'Basic realm="World News administration", charset="UTF-8"' } : {}),
    },
  });
}

export const config = { matcher: ['/admin/:path*'] };
