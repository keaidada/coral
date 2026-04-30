'use client';

import { useMemo, useState } from 'react';
import {
  Play,
  Loader2,
  Copy,
  Check,
  Download,
  Workflow,
  ExternalLink,
  Wand2,
  ChevronDown,
  ChevronRight as ChevRight,
} from 'lucide-react';
import LZString from 'lz-string';
import {
  Card,
  CardBody,
  CardHeader,
  PageHeader,
  Button,
  Select,
  Pill,
  CodeBlock,
} from '@/app/components/ui';
import SqlEditor from '@/app/components/SqlEditor';
import { API_BASE, formatSql } from '@/app/lib/client';
import { useT } from '@/app/lib/i18n';

const SAMPLE = `SELECT a FROM t WHERE id > 10`;

// Build the GraphvizOnline URL. Docs:
//   https://dreampuf.github.io/GraphvizOnline/
// It reads a ?compressed=<lz-string-encoded> query param and renders
// the DOT source immediately. This is the same scheme used by their
// "Share URL" button.
function graphvizOnlineUrl(dot) {
  const c = LZString.compressToEncodedURIComponent(dot);
  return `https://dreampuf.github.io/GraphvizOnline/?compressed=${c}#graph0`;
}

// PlantUML's server accepts raw hex source via `~h<hex>`. That
// bypasses the DEFLATE encoding scheme and lets us embed the text
// directly. Safe for the small SQL trees we emit.
function plantumlUrl(src) {
  const hex = Array.from(new TextEncoder().encode(src))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
  return `https://www.plantuml.com/plantuml/uml/~h${hex}`;
}

export default function VisualizePage() {
  const t = useT();
  const [query, setQuery] = useState(SAMPLE);
  const [format, setFormat] = useState('dot');
  const [loading, setLoading] = useState(false);
  const [source, setSource] = useState(null);
  const [error, setError] = useState(null);
  const [formatError, setFormatError] = useState(null);
  const [copied, setCopied] = useState(false);
  const [showSource, setShowSource] = useState(false);

  const previewUrl = useMemo(() => {
    if (!source) return null;
    return format === 'plantuml'
      ? plantumlUrl(source)
      : graphvizOnlineUrl(source);
  }, [source, format]);

  function onFormat() {
    if (!query.trim()) return;
    try {
      setQuery(formatSql(query, 'postgresql'));
      setFormatError(null);
    } catch (err) {
      setFormatError(String(err.message || err));
      setTimeout(() => setFormatError(null), 4000);
    }
  }

  async function onSubmit(e) {
    e?.preventDefault?.();
    setLoading(true);
    setError(null);
    setSource(null);
    try {
      const gen = await fetch(API_BASE + '/api/visualizations/generategraphs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query, format }),
      });
      if (!gen.ok) {
        const j = await gen.json().catch(() => ({}));
        throw new Error(j.error || `HTTP ${gen.status}`);
      }
      const meta = await gen.json();
      const resp = await fetch(
        API_BASE + `/api/visualizations/${meta.graphId}`,
      );
      const text = await resp.text();
      setSource(text);
    } catch (err) {
      setError(String(err.message || err));
    } finally {
      setLoading(false);
    }
  }

  async function copySource() {
    await navigator.clipboard.writeText(source || '');
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  function download() {
    const ext = format === 'plantuml' ? 'puml' : 'dot';
    const blob = new Blob([source || ''], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `coral-ast.${ext}`;
    a.click();
    URL.revokeObjectURL(url);
  }

  return (
    <div>
      <PageHeader
        title={t('visualize.title')}
        subtitle={t('visualize.subtitle')}
      />

      <Card>
        <CardHeader>
          <Pill tone='coral'>{t('visualize.ast')}</Pill>
          <span className='text-xs text-ink-500'>{t('visualize.format')}</span>
          <Select value={format} onChange={(e) => setFormat(e.target.value)}>
            <option value='dot'>{t('visualize.dot')}</option>
            <option value='plantuml'>{t('visualize.plantuml')}</option>
          </Select>
          <span className='ml-auto text-xs text-ink-500'>
            {t('visualize.hint')}
          </span>
        </CardHeader>
        <SqlEditor
          value={query}
          onChange={setQuery}
          dialect='postgresql'
          onSubmit={onSubmit}
          onFormat={onFormat}
          minHeight='180px'
        />
        {formatError && (
          <div className='px-5 py-2 text-xs text-amber-700 dark:text-amber-400 bg-amber-50/70 dark:bg-amber-900/20 border-t border-amber-200 dark:border-amber-800/40'>
            {t('translate.formatFailed')}
          </div>
        )}
        <div className='px-5 py-3 border-t border-ink-200 dark:border-ink-800 flex items-center gap-3 bg-ink-50/40 dark:bg-ink-950/40'>
          <Button
            type='button'
            variant='ghost'
            size='sm'
            onClick={onFormat}
            disabled={!query.trim()}
            title='⇧⌘F'
          >
            <Wand2 size={14} />
            {t('translate.format')}
          </Button>
          <span className='ml-auto' />
          <Button
            type='button'
            onClick={onSubmit}
            disabled={loading || !query.trim()}
          >
            {loading ? (
              <>
                <Loader2 size={14} className='animate-spin' />
                {t('visualize.running')}
              </>
            ) : (
              <>
                <Play size={14} />
                {t('visualize.run')}
              </>
            )}
          </Button>
        </div>
      </Card>

      {error && (
        <Card className='mt-5 border-red-200 dark:border-red-800/60'>
          <CardBody>
            <div className='text-sm text-red-700 dark:text-red-400 font-medium mb-2'>
              {t('visualize.failed')}
            </div>
            <CodeBlock className='!bg-red-50/50 dark:!bg-red-950/30 !border-red-200 dark:!border-red-900/40'>
              {error}
            </CodeBlock>
          </CardBody>
        </Card>
      )}

      {source && previewUrl && (
        <Card className='mt-5 overflow-hidden'>
          <CardHeader>
            <Workflow size={16} className='text-coral-600' />
            <span className='font-medium'>{t('visualize.preview')}</span>
            <Pill tone='neutral' className='font-mono'>
              {format}
            </Pill>
            <span className='text-[11px] text-ink-500 ml-1'>
              {t('visualize.previewIn')}{' '}
              {format === 'plantuml'
                ? 'plantuml.com'
                : 'dreampuf.github.io/GraphvizOnline'}
            </span>
            <div className='ml-auto flex items-center gap-2'>
              <a
                href={previewUrl}
                target='_blank'
                rel='noreferrer'
                className='inline-flex items-center gap-1.5 text-xs px-2.5 py-1.5 rounded-md border border-ink-200 dark:border-ink-800 text-ink-700 dark:text-ink-300 hover:bg-ink-50 dark:hover:bg-ink-800 transition'
              >
                <ExternalLink size={13} />
                {t('visualize.openInNewTab')}
              </a>
            </div>
          </CardHeader>
          <div className='bg-white dark:bg-ink-950'>
            <iframe
              title='AST preview'
              src={previewUrl}
              className='block w-full'
              style={{ height: '640px', border: 0 }}
              sandbox='allow-scripts allow-same-origin allow-popups allow-forms'
              referrerPolicy='no-referrer'
            />
          </div>

          {/* Collapsible raw source */}
          <button
            type='button'
            onClick={() => setShowSource((v) => !v)}
            className='w-full px-5 py-3 border-t border-ink-200 dark:border-ink-800 flex items-center gap-2 text-sm text-ink-600 dark:text-ink-300 hover:bg-ink-50/60 dark:hover:bg-ink-950/60 transition'
          >
            {showSource ? <ChevronDown size={14} /> : <ChevRight size={14} />}
            <span className='font-medium'>{t('visualize.graphSource')}</span>
            <span className='ml-2 text-[11px] text-ink-500 font-mono'>
              {source.length} chars
            </span>
            <div
              className='ml-auto flex items-center gap-2'
              onClick={(e) => e.stopPropagation()}
            >
              <Button variant='secondary' size='sm' onClick={copySource}>
                {copied ? (
                  <>
                    <Check size={13} /> {t('visualize.copied')}
                  </>
                ) : (
                  <>
                    <Copy size={13} /> {t('visualize.copy')}
                  </>
                )}
              </Button>
              <Button variant='secondary' size='sm' onClick={download}>
                <Download size={13} />
                {t('visualize.download')}
              </Button>
            </div>
          </button>
          {showSource && (
            <pre className='font-mono text-[12px] leading-relaxed p-5 overflow-auto whitespace-pre bg-ink-50/40 dark:bg-ink-950/40 text-ink-800 dark:text-ink-100 max-h-[420px] border-t border-ink-200 dark:border-ink-800'>
              {source}
            </pre>
          )}
        </Card>
      )}
    </div>
  );
}
