'use client';

import CodeMirror, { keymap } from '@uiw/react-codemirror';
import { sql, PostgreSQL, MySQL, StandardSQL } from '@codemirror/lang-sql';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { oneDark } from '@codemirror/theme-one-dark';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';
import { useEffect, useMemo, useState } from 'react';

// Pick a dialect object based on the user-selected source language.
// sql-lang-codemirror doesn't ship Hive/Trino/Spark explicitly — they
// are close enough to PostgreSQL / StandardSQL for highlighting
// purposes. Keyword coverage is approximate; this is about visual
// pleasantness, not validation (validation lives on the backend).
function dialectFor(lang) {
  switch (lang) {
    case 'gaussdb':
    case 'postgres':
    case 'postgresql':
      return PostgreSQL;
    case 'mysql':
      return MySQL;
    default:
      return StandardSQL;
  }
}

const coralLightHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: '#c73826', fontWeight: '600' },
  { tag: tags.operatorKeyword, color: '#c73826', fontWeight: '600' },
  { tag: tags.function(tags.variableName), color: '#0f766e' },
  { tag: tags.variableName, color: '#404040' },
  { tag: tags.string, color: '#b45309' },
  { tag: tags.number, color: '#2563eb' },
  { tag: tags.bool, color: '#7c3aed' },
  { tag: tags.null, color: '#7c3aed' },
  { tag: tags.comment, color: '#737373', fontStyle: 'italic' },
  { tag: tags.operator, color: '#525252' },
  { tag: tags.punctuation, color: '#737373' },
]);

const coralDarkHighlight = HighlightStyle.define([
  { tag: tags.keyword, color: '#ff8a78', fontWeight: '600' },
  { tag: tags.operatorKeyword, color: '#ff8a78', fontWeight: '600' },
  { tag: tags.function(tags.variableName), color: '#5eead4' },
  { tag: tags.variableName, color: '#e5e5e5' },
  { tag: tags.string, color: '#fbbf24' },
  { tag: tags.number, color: '#93c5fd' },
  { tag: tags.bool, color: '#c4b5fd' },
  { tag: tags.null, color: '#c4b5fd' },
  { tag: tags.comment, color: '#a3a3a3', fontStyle: 'italic' },
  { tag: tags.operator, color: '#d4d4d4' },
  { tag: tags.punctuation, color: '#a3a3a3' },
]);

// Base editor extensions shared across all instances.
const BASE_EXTENSIONS = [
  EditorView.lineWrapping,
  EditorView.theme({
    '&': { fontSize: '13px' },
    '.cm-content': {
      fontFamily:
        "'JetBrains Mono', ui-monospace, SFMono-Regular, Menlo, monospace",
      lineHeight: '1.65',
      padding: '16px 20px',
    },
    '.cm-gutters': {
      backgroundColor: 'transparent',
      border: 'none',
      color: 'rgb(163 163 163)',
      paddingLeft: '8px',
    },
    '.cm-focused': { outline: 'none' },
    '.cm-activeLine': { backgroundColor: 'rgba(255,107,91,0.04)' },
    '.cm-activeLineGutter': { backgroundColor: 'transparent' },
  }),
];

export default function SqlEditor({
  value,
  onChange,
  dialect = 'postgresql',
  onSubmit,
  onFormat,
  minHeight = '240px',
  readOnly = false,
  placeholder,
}) {
  const [dark, setDark] = useState(false);

  useEffect(() => {
    const check = () =>
      setDark(document.documentElement.classList.contains('dark'));
    check();
    // Watch the <html> class attribute for theme changes so the editor
    // swaps styles live (Shell toggles .dark on <html>).
    const mo = new MutationObserver(check);
    mo.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });
    return () => mo.disconnect();
  }, []);

  const extensions = useMemo(() => {
    const exts = [
      sql({
        dialect: dialectFor(dialect),
        upperCaseKeywords: true,
      }),
      syntaxHighlighting(dark ? coralDarkHighlight : coralLightHighlight),
      ...BASE_EXTENSIONS,
    ];
    const keys = [];
    if (onSubmit) {
      keys.push({
        key: 'Mod-Enter',
        preventDefault: true,
        run: () => {
          onSubmit();
          return true;
        },
      });
    }
    if (onFormat) {
      keys.push({
        key: 'Shift-Mod-f',
        preventDefault: true,
        run: () => {
          onFormat();
          return true;
        },
      });
    }
    if (keys.length) exts.push(keymap.of(keys));
    return exts;
  }, [dark, dialect, onSubmit, onFormat]);

  return (
    <div
      className='cm-coral-wrapper'
      style={{ minHeight }}
      data-readonly={readOnly || undefined}
    >
      <CodeMirror
        value={value}
        onChange={onChange}
        extensions={extensions}
        theme={dark ? oneDark : 'light'}
        basicSetup={{
          lineNumbers: true,
          highlightActiveLine: true,
          highlightActiveLineGutter: true,
          foldGutter: false,
          autocompletion: true,
          indentOnInput: true,
          bracketMatching: true,
          closeBrackets: true,
          drawSelection: true,
        }}
        readOnly={readOnly}
        placeholder={placeholder}
        minHeight={minHeight}
      />
    </div>
  );
}
