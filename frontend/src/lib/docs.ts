// @ts-nocheck

function normalizeDocPath(path) {
    if (!path) return '';
    let normalized = path.trim();
    normalized = normalized.replace(/^\.?\//, '');
    normalized = normalized.replace(/^docs\//, '');
    if (normalized === '../README.md' || normalized === './README.md') {
        return 'README.md';
    }
    while (normalized.startsWith('../')) normalized = normalized.slice(3);
    return normalized;
}

export function slugFromDocPath(docPath) {
    const normalized = normalizeDocPath(docPath);
    if (!normalized) return '';
    if (normalized.toLowerCase() === 'readme.md') return 'overview';
    const withoutExt = normalized.replace(/\.md$/i, '');
    if (withoutExt.endsWith('/index')) {
        return withoutExt.slice(0, -('/index'.length));
    }
    return withoutExt;
}

export function docPathFromSlug(slug) {
    if (!slug) return '';
    if (slug === 'overview') return 'README.md';
    if (slug.includes('/')) {
        return `${slug}.md`;
    }
    return `${slug}.md`;
}

export function titleFromSlug(slug) {
    if (!slug || slug === 'overview') return 'Overview';
    const leaf = slug.split('/').pop() || slug;
    return leaf
        .split('-')
        .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
        .join(' ');
}

export function parseDocsSummary(summaryMarkdown) {
    const entries = [];
    const lines = summaryMarkdown.split('\n');

    for (const line of lines) {
        const match = line.match(/^(\s*)-\s*\[([^\]]+)\]\(([^)]+)\)/);
        if (!match) continue;

        const indent = match[1].length;
        const title = match[2].trim();
        const path = normalizeDocPath(match[3].trim());

        if (path.toLowerCase().endsWith('.md')) {
            const depth = Math.floor(indent / 2);
            entries.push({ title, path, slug: slugFromDocPath(path), depth });
        }
    }

    return entries;
}
