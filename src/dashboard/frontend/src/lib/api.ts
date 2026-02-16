import type { HttpMethod } from './types';
import { auth } from './state.svelte';

export class UnauthorizedError extends Error {
    constructor() {
        super('Unauthorized');
        this.name = 'UnauthorizedError';
    }
}

export async function api<T = unknown>(
    method: HttpMethod,
    path: string,
    body?: Record<string, unknown>,
): Promise<T> {
    const opts: RequestInit = { method, headers: {} };

    if (body) {
        (opts.headers as Record<string, string>)['Content-Type'] = 'application/json';
        opts.body = JSON.stringify(body);
    }

    const res = await fetch(path, opts);

    if (res.status === 401) {
        auth.authenticated = false;
        throw new UnauthorizedError();
    }

    if (!res.ok) throw new Error(`${method} ${path}: ${res.status}`);
    return res.json() as Promise<T>;
}
