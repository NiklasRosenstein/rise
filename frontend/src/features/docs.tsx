// @ts-nocheck
import { useEffect, useMemo, useState } from 'react';
import { marked } from 'marked';
import { navigate } from '../lib/navigation';
import { extensionTypeFromDocPath, parseDocsSummary, slugFromDocPath } from '../lib/docs';
import { ErrorState, LoadingState } from '../components/states';
import { CONFIG } from '../lib/config';
import { api } from '../lib/api';

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

function ingressSuffix(template, lastPlaceholder) {
    const idx = template.lastIndexOf(lastPlaceholder);
    if (idx === -1) return null;
    return template.substring(idx + lastPlaceholder.length);
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
    const [availableExtensionTypes, setAvailableExtensionTypes] = useState<string[]>([]);
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
            const [summaryResponse, extensionTypesResponse] = await Promise.all([
                fetch(`${docsBasePath}/FRONTEND_DOCS.md`),
                api.getExtensionTypes().catch(() => ({ extension_types: [] })),
            ]);
            if (!summaryResponse.ok) throw new Error(`Failed to load summary (${summaryResponse.status})`);

            const markdown = await summaryResponse.text();
            setSummaryItems(parseDocsSummary(markdown));
            setAvailableExtensionTypes((extensionTypesResponse.extension_types || []).map((ext) => ext.extension_type));
        } catch (err) {
            setSummaryError(err.message);
        } finally {
            setSummaryLoading(false);
        }
    };

    useEffect(() => {
        loadSummary();
    }, []);

    const hasExplicitSlug = Boolean((activeSlug || '').trim());
    const effectiveSlug = useMemo(() => {
        if (!summaryItems.length) return activeSlug || '';
        if (hasExplicitSlug) return activeSlug;
        return summaryItems[0].slug;
    }, [activeSlug, hasExplicitSlug, summaryItems]);

    const effectiveDocEntry = useMemo(() => {
        if (!effectiveSlug) return null;
        return summaryItems.find((item) => item.slug === effectiveSlug) || null;
    }, [effectiveSlug, summaryItems]);

    const extensionDocEntries = useMemo(() => {
        const available = new Set(availableExtensionTypes);
        return summaryItems
            .filter((item) => extensionTypeFromDocPath(item.path))
            .map((item) => {
                const extensionType = extensionTypeFromDocPath(item.path);
                return {
                    ...item,
                    extensionType,
                    available: available.has(extensionType),
                };
            });
    }, [summaryItems, availableExtensionTypes]);

    const loadDoc = async () => {
        if (!effectiveSlug) {
            setDocHtml('');
            setDocLoading(false);
            setDocError('No User Guide pages found in docs summary.');
            return;
        }
        if (!effectiveDocEntry) {
            setDocHtml('');
            setDocLoading(false);
            setDocError('Documentation page not found.');
            return;
        }

        const extensionType = extensionTypeFromDocPath(effectiveDocEntry.path);
        if (extensionType && !availableExtensionTypes.includes(extensionType)) {
            setDocHtml('');
            setDocLoading(false);
            setDocError('Documentation for this extension is not enabled.');
            return;
        }

        setDocLoading(true);
        setDocError(null);
        try {
            const response = await fetch(`${docsBasePath}/${effectiveDocEntry.path}`);
            if (!response.ok) throw new Error(`Failed to load document (${response.status})`);
            const markdown = await response.text();
            const lowered = markdown.trimStart().toLowerCase();
            if (lowered.startsWith('<!doctype html') || lowered.startsWith('<html') || markdown.includes('src="/@vite/client"')) {
                throw new Error('Documentation content is unavailable (received HTML fallback instead of markdown).');
            }
            let processed = markdown.replaceAll('https://rise.example.com', CONFIG.backendUrl);
            if (CONFIG.productionIngressUrlTemplate) {
                const suffix = ingressSuffix(CONFIG.productionIngressUrlTemplate, '{project_name}');
                if (suffix) processed = processed.replaceAll('.app.example.com', suffix);
            }
            if (CONFIG.stagingIngressUrlTemplate) {
                const suffix = ingressSuffix(CONFIG.stagingIngressUrlTemplate, '{deployment_group}');
                if (suffix) processed = processed.replaceAll('.preview.example.com', suffix);
            }
            const html = rewriteMarkdownLinks(processed, effectiveDocEntry.path);
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
    }, [effectiveSlug, effectiveDocEntry, availableExtensionTypes.join(',')]);

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
                    <>
                        <div className="prose max-w-none" onClick={onDocContentClick} dangerouslySetInnerHTML={{ __html: docHtml }} />
                        {effectiveSlug === 'extensions' && extensionDocEntries.length > 0 && (
                            <div className="mt-6 pt-4 border-t border-gray-300 dark:border-gray-700">
                                <h3 className="text-sm font-semibold text-gray-900 dark:text-gray-200 mb-3">Extension Documentation</h3>
                                <ul className="list-disc list-inside space-y-2">
                                    {extensionDocEntries.map((item) => (
                                        <li key={item.slug} className="text-sm">
                                            {item.available ? (
                                                <a
                                                    href={`/docs/${item.slug}`}
                                                    onClick={(e) => {
                                                        e.preventDefault();
                                                        navigate(`/docs/${item.slug}`);
                                                    }}
                                                    className="underline"
                                                >
                                                    {item.title}
                                                </a>
                                            ) : (
                                                <span className="text-gray-500 dark:text-gray-500">
                                                    {item.title} <span className="text-xs">(Not enabled)</span>
                                                </span>
                                            )}
                                        </li>
                                    ))}
                                </ul>
                            </div>
                        )}
                    </>
                )}
            </article>
        </section>
    );
}
