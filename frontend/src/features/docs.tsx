// @ts-nocheck
import { useEffect, useMemo, useState } from 'react';
import { marked } from 'marked';
import { navigate } from '../lib/navigation';
import { parseDocsSummary, slugFromDocPath } from '../lib/docs';
import { ErrorState, LoadingState } from '../components/states';

function resolveRelativePath(basePath, hrefPath) {
    const baseParts = basePath.split('/');
    baseParts.pop();
    const hrefParts = hrefPath.split('/');
    const out = [...baseParts];

    for (const part of hrefParts) {
        if (!part || part === '.') continue;
        if (part === '..') {
            out.pop();
            continue;
        }
        out.push(part);
    }

    return out.join('/');
}

function rewriteMarkdownLinks(markdown, currentDocPath) {
    const rewritten = markdown.replace(/(?<!!)\[([^\]]+)\]\(([^)]+)\)/g, (full, text, href) => {
        const rawHref = (href || '').trim();
        if (!rawHref || rawHref.startsWith('#') || /^https?:\/\//i.test(rawHref) || rawHref.startsWith('mailto:')) {
            return full;
        }

        const [pathPart, hashPart] = rawHref.split('#');
        const resolved = resolveRelativePath(currentDocPath || 'README.md', pathPart);
        if (!resolved.toLowerCase().endsWith('.md')) {
            return full;
        }

        const slug = slugFromDocPath(resolved);
        const target = `/docs/${slug}${hashPart ? `#${hashPart}` : ''}`;
        return `[${text}](${target})`;
    });

    return marked.parse(rewritten);
}

function escapeHtml(value) {
    return value
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;');
}

function highlightCode(code, language) {
    const lang = (language || '').toLowerCase();
    const keywordSets = {
        js: ['const', 'let', 'var', 'function', 'return', 'if', 'else', 'for', 'while', 'await', 'async', 'import', 'from', 'export', 'class', 'new', 'try', 'catch', 'throw'],
        ts: ['const', 'let', 'var', 'function', 'return', 'if', 'else', 'for', 'while', 'await', 'async', 'import', 'from', 'export', 'class', 'new', 'type', 'interface', 'extends', 'implements'],
        python: ['def', 'return', 'if', 'elif', 'else', 'for', 'while', 'import', 'from', 'class', 'try', 'except', 'raise', 'with', 'as', 'lambda'],
        rust: ['fn', 'let', 'mut', 'if', 'else', 'for', 'while', 'match', 'struct', 'enum', 'impl', 'trait', 'pub', 'use', 'mod', 'return', 'async', 'await'],
        bash: ['if', 'then', 'else', 'fi', 'for', 'do', 'done', 'case', 'esac', 'function', 'export', 'local', 'return'],
        sh: ['if', 'then', 'else', 'fi', 'for', 'do', 'done', 'case', 'esac', 'function', 'export', 'local', 'return'],
        json: [],
        yaml: [],
        yml: [],
        toml: [],
    };

    const keywords = keywordSets[lang] || [];
    let out = escapeHtml(code);

    const placeholders = [];
    const store = (match, cls) => {
        const token = `__TK_${placeholders.length}__`;
        placeholders.push(`<span class="mono-hl-${cls}">${match}</span>`);
        return token;
    };

    // Strings first.
    out = out.replace(/"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/g, (m) => store(m, 'string'));

    // Comments.
    if (lang === 'python' || lang === 'bash' || lang === 'sh' || lang === 'yaml' || lang === 'yml' || lang === 'toml') {
        out = out.replace(/(^|\s)#.*$/gm, (m) => store(m, 'comment'));
    } else {
        out = out.replace(/\/\/.*$/gm, (m) => store(m, 'comment'));
        out = out.replace(/\/\*[\s\S]*?\*\//g, (m) => store(m, 'comment'));
    }

    // Numbers.
    out = out.replace(/\b\d+(?:\.\d+)?\b/g, (m) => `<span class="mono-hl-number">${m}</span>`);

    // Keywords.
    if (keywords.length > 0) {
        const kw = new RegExp(`\\b(${keywords.join('|')})\\b`, 'g');
        out = out.replace(kw, (_, m) => `<span class="mono-hl-keyword">${m}</span>`);
    }

    // Restore placeholders.
    out = out.replace(/__TK_(\d+)__/g, (_, idx) => placeholders[Number(idx)] || '');
    return out;
}

export function DocsPage({ initialSlug }) {
    const [summaryItems, setSummaryItems] = useState([]);
    const [summaryLoading, setSummaryLoading] = useState(true);
    const [summaryError, setSummaryError] = useState(null);
    const [docHtml, setDocHtml] = useState('');
    const [docLoading, setDocLoading] = useState(true);
    const [docError, setDocError] = useState(null);
    const [activeSlug, setActiveSlug] = useState(initialSlug || '');
    const [highlightVersion, setHighlightVersion] = useState(0);

    const docsBasePath = '/static/docs';

    useEffect(() => {
        setActiveSlug(initialSlug || '');
    }, [initialSlug]);

    const loadSummary = async () => {
        setSummaryLoading(true);
        setSummaryError(null);
        try {
            const response = await fetch(`${docsBasePath}/FRONTEND_DOCS.md`);
            if (!response.ok) throw new Error(`Failed to load summary (${response.status})`);
            const markdown = await response.text();
            const items = parseDocsSummary(markdown);
            setSummaryItems(items);
        } catch (err) {
            setSummaryError(err.message);
        } finally {
            setSummaryLoading(false);
        }
    };

    useEffect(() => {
        loadSummary();
    }, []);

    const effectiveSlug = useMemo(() => {
        if (!summaryItems.length) return activeSlug || '';
        if (activeSlug && summaryItems.some((item) => item.slug === activeSlug)) return activeSlug;
        return summaryItems[0].slug;
    }, [activeSlug, summaryItems]);

    const effectiveDocPath = useMemo(() => {
        if (!effectiveSlug) return '';
        const fromSummary = summaryItems.find((item) => item.slug === effectiveSlug);
        if (!fromSummary) return summaryItems[0]?.path || '';
        return fromSummary.path;
    }, [effectiveSlug, summaryItems]);

    const loadDoc = async () => {
        if (!effectiveDocPath) {
            setDocHtml('');
            setDocLoading(false);
            setDocError('No User Guide pages found in docs summary.');
            return;
        }
        setDocLoading(true);
        setDocError(null);
        try {
            const response = await fetch(`${docsBasePath}/${effectiveDocPath}`);
            if (!response.ok) throw new Error(`Failed to load document (${response.status})`);
            const markdown = await response.text();
            const lowered = markdown.trimStart().toLowerCase();
            if (lowered.startsWith('<!doctype html') || lowered.startsWith('<html') || markdown.includes('src="/@vite/client"')) {
                throw new Error('Documentation content is unavailable (received HTML fallback instead of markdown).');
            }
            const html = rewriteMarkdownLinks(markdown, effectiveDocPath);
            setDocHtml(html);
            setHighlightVersion((v) => v + 1);
        } catch (err) {
            setDocError(err.message);
        } finally {
            setDocLoading(false);
        }
    };

    useEffect(() => {
        loadDoc();
    }, [effectiveDocPath]);

    useEffect(() => {
        if (!docHtml) return;
        const blocks = document.querySelectorAll('.mono-docs-content pre code');
        blocks.forEach((block) => {
            const classNames = block.className || '';
            const language = (classNames.match(/language-([a-z0-9_-]+)/i)?.[1] || '').toLowerCase();
            const raw = block.textContent || '';
            block.innerHTML = highlightCode(raw, language);
            block.classList.add('mono-hl');
        });
    }, [highlightVersion, docHtml]);

    const onDocContentClick = (event) => {
        const link = event.target?.closest?.('a');
        if (!link) return;
        const href = link.getAttribute('href') || '';
        if (!href.startsWith('/docs/')) return;
        event.preventDefault();
        navigate(href);
    };

    if (summaryLoading) return <LoadingState label="Loading documentation..." />;
    if (summaryError) return <ErrorState message={`Error loading docs summary: ${summaryError}`} onRetry={loadSummary} />;

    return (
        <section className="mono-docs">
            <article className="mono-docs-content">
                {docLoading ? (
                    <LoadingState label="Loading page..." />
                ) : docError ? (
                    <ErrorState message={`Error loading document: ${docError}`} onRetry={loadDoc} />
                ) : (
                    <div className="prose max-w-none" onClick={onDocContentClick} dangerouslySetInnerHTML={{ __html: docHtml }} />
                )}
            </article>
        </section>
    );
}
