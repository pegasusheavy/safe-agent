/**
 * Shared timestamp formatting utilities.
 *
 * All timestamps from the backend are stored in UTC (SQLite `datetime('now')`
 * produces `YYYY-MM-DD HH:MM:SS` in UTC; API responses use RFC 3339).
 *
 * The formatting functions here parse UTC strings and display them in the
 * browser's local timezone/locale (which respects the OS-level settings and
 * the user's configured timezone when the browser matches it).
 */

/**
 * Parse a timestamp string from the backend into a Date object.
 * Handles both RFC 3339 (`2025-02-17T12:34:56+00:00`) and SQLite
 * datetime format (`2025-02-17 12:34:56`).
 */
export function parseUtc(ts: string): Date | null {
    if (!ts) return null;
    try {
        // If it looks like a bare SQLite datetime (no timezone indicator),
        // append 'Z' to treat it as UTC.
        if (/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/.test(ts)) {
            return new Date(ts.replace(' ', 'T') + 'Z');
        }
        return new Date(ts);
    } catch {
        return null;
    }
}

/**
 * Format a backend timestamp as a short time string (e.g. "2:34 PM").
 */
export function formatTime(ts: string, locale?: string): string {
    const d = parseUtc(ts);
    if (!d || isNaN(d.getTime())) return '';
    return d.toLocaleTimeString(locale || undefined, {
        hour: '2-digit',
        minute: '2-digit',
    });
}

/**
 * Format a backend timestamp as a date + time string
 * (e.g. "Feb 17, 2025, 2:34 PM").
 */
export function formatDateTime(ts: string, locale?: string): string {
    const d = parseUtc(ts);
    if (!d || isNaN(d.getTime())) return ts; // fallback to raw string
    return d.toLocaleString(locale || undefined, {
        month: 'short',
        day: 'numeric',
        year: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
    });
}

/**
 * Format a backend timestamp as a relative-time label
 * ("Today 2:34 PM", "Yesterday 10:00 AM", "Feb 14, 2025 8:00 PM").
 */
export function formatRelative(ts: string, locale?: string): string {
    const d = parseUtc(ts);
    if (!d || isNaN(d.getTime())) return ts;

    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const yesterday = new Date(today.getTime() - 86_400_000);
    const dateOnly = new Date(d.getFullYear(), d.getMonth(), d.getDate());
    const timeStr = d.toLocaleTimeString(locale || undefined, {
        hour: '2-digit',
        minute: '2-digit',
    });

    if (dateOnly.getTime() === today.getTime()) {
        return `Today ${timeStr}`;
    }
    if (dateOnly.getTime() === yesterday.getTime()) {
        return `Yesterday ${timeStr}`;
    }
    return formatDateTime(ts, locale);
}
