'use client';

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from 'react';

const DICT = {
  en: {
    // Nav
    'nav.translate': 'Translate',
    'nav.validate': 'Validate',
    'nav.visualize': 'Visualize',
    'nav.history': 'History',
    'nav.functions': 'Functions',
    'nav.settings': 'Settings',

    // Shell / topbar
    'shell.search': 'Search or jump to…',
    'shell.connected': 'Connected',
    'shell.toggleTheme': 'Toggle theme',
    'shell.toggleLang': 'Switch language',

    // Translate page
    'translate.title': 'Translate',
    'translate.subtitle':
      'Convert SQL between Hive, Spark, Trino, and GaussDB dialects.',
    'translate.newTranslation': 'New translation',
    'translate.from': 'From',
    'translate.to': 'To',
    'translate.rewrite': 'Rewrite',
    'translate.rewriteNone': 'None',
    'translate.rewriteIncremental': 'Incremental',
    'translate.placeholder': 'Paste SQL here…',
    'translate.hint': '⌘↵ to translate',
    'translate.clear': 'Clear',
    'translate.format': 'Format',
    'translate.formatFailed': 'Could not format — invalid SQL?',
    'translate.openRecent': 'Recent',
    'translate.run': 'Translate',
    'translate.running': 'Translating…',
    'translate.chars': '{n} chars',
    'translate.tokens': '{n} tokens',
    'translate.failed': 'Translation failed',
    'translate.success': 'Success',
    'translate.copy': 'Copy',
    'translate.copied': 'Copied',
    'translate.translated': 'Translated',
    'translate.recent': 'Recent',
    'translate.viewAll': 'View all',
    'translate.noHistory':
      'No history yet. Run a translation above to start building it.',

    // History page
    'history.title': 'History',
    'history.subtitle':
      '{n} saved translation(s). Stored locally in your browser.',
    'history.clear': 'Clear',
    'history.clearConfirm': 'Clear all translation history?',
    'history.filter': 'Filter by SQL, source, target…',
    'history.favsOnly': 'Favorites only',
    'history.empty': 'No translations yet. Run one from the Translate page.',
    'history.noMatch': 'No results for this filter.',
    'history.rerun': 'Rerun',
    'history.delete': 'Delete',
    'history.source': 'Source',
    'history.translated': 'Translated',

    // Functions page
    'functions.title': 'Functions',
    'functions.subtitleLoading': 'Loading registry…',
    'functions.subtitle':
      '{n} functions in registry. Filter, search, or cross-reference translations.',
    'functions.search': 'Search by name, notes, category…',
    'functions.disposition': 'Disposition',
    'functions.all': 'All',
    'functions.loading': 'Loading…',
    'functions.failed': 'Failed to load functions: {err}',
    'functions.colName': 'Name',
    'functions.colDisposition': 'Disposition',
    'functions.colCategory': 'Category',
    'functions.colNotes': 'Notes',
    'functions.noMatch': 'No matches.',

    // Validate page
    'validate.title': 'Validate',
    'validate.subtitle':
      "Parse SQL and report whether it's syntactically valid. No execution, no catalog required.",
    'validate.parseCheck': 'Parse check',
    'validate.hint': '⌘↵ to validate',
    'validate.placeholder': 'Paste SQL…',
    'validate.run': 'Validate',
    'validate.running': 'Validating…',
    'validate.failed': 'Request failed',
    'validate.ok': 'Parses cleanly',
    'validate.okDetail':
      "The SQL is syntactically valid for coral's PostgreSQL-based parser.",
    'validate.statementCount': '{n} statement(s)',
    'validate.parseError': 'Parse error',
    'validate.unknownError': 'Unknown parse error',

    // Visualize page
    'visualize.title': 'Visualize',
    'visualize.subtitle':
      'Render the SQL parse tree in an embedded GraphvizOnline preview or PlantUML.',
    'visualize.ast': 'AST',
    'visualize.format': 'Format',
    'visualize.dot': 'Graphviz (DOT)',
    'visualize.plantuml': 'PlantUML',
    'visualize.hint': '⌘↵ to render',
    'visualize.run': 'Render',
    'visualize.running': 'Rendering…',
    'visualize.failed': 'Render failed',
    'visualize.graphSource': 'Graph source',
    'visualize.preview': 'Preview',
    'visualize.previewIn': 'Rendered via',
    'visualize.openInNewTab': 'Open in new tab',
    'visualize.copy': 'Copy',
    'visualize.copied': 'Copied',
    'visualize.download': 'Download',
    'visualize.tipBefore': 'Tip: paste into',
    'visualize.tipOr': 'or',
    'visualize.tipAfter': 'to view.',

    // Settings page
    'settings.title': 'Settings',
    'settings.subtitle':
      'Theme, language, backend info, and local catalog for development.',
    'settings.tabGeneral': 'General',
    'settings.tabCatalog': 'Catalog (local)',
    'settings.tabAbout': 'About',
    'settings.theme': 'Theme',
    'settings.themeDesc': 'Choose a light or dark interface.',
    'settings.light': 'Light',
    'settings.dark': 'Dark',
    'settings.language': 'Language',
    'settings.languageDesc': 'Choose the interface language.',
    'settings.backend': 'Backend',
    'settings.backendDesc': 'API base URL',
    'settings.backendUnset':
      '(same origin — NEXT_PUBLIC_CORAL_SERVICE_API_URL unset)',
    'settings.catalogTitle': 'Create database / table / view',
    'settings.catalogLocalOnly': 'Local mode only',
    'settings.catalogPlaceholder': 'CREATE DATABASE / TABLE / VIEW …',
    'settings.catalogHint':
      'Runs against the in-process catalog. Useful for validating translations against an explicit schema.',
    'settings.execute': 'Execute',
    'settings.executing': 'Executing…',
    'settings.catalogFailed': 'Failed',
    'settings.catalogSuccess': 'Creation successful',
    'settings.response': 'Response',
    'settings.aboutSub': 'coral-rust workspace · v0.3',
    'settings.aboutDesc':
      'Coral is a SQL translation, validation, and visualization toolchain. This frontend talks to the Rust backend (coral-service) and supports Hive, Spark, Trino, and GaussDB dialects. Every translation is cached locally in your browser — nothing leaves the machine unless the backend is remote.',
    'settings.statFn': 'Function registry',
    'settings.statParser': 'Parse engine',
    'settings.statPretty': 'Pretty-printer',
    'settings.statTargets': 'Targets',
    'settings.statFnV': '331 entries',
    'settings.statParserV': 'sqlparser-rs / PostgreSQL',
    'settings.statPrettyV': 'Calcite-style',
    'settings.statTargetsV': 'Spark · Trino',

    // Command palette
    'cmd.placeholder': 'Type a command or search history…',
    'cmd.empty': 'No results.',
    'cmd.navigate': 'Navigate',
    'cmd.recent': 'Recent translations',
    'cmd.actions': 'Actions',
    'cmd.toggleTheme': 'Toggle theme',
    'cmd.toggleLang': 'Switch language',
    'cmd.clearHistory': 'Clear history',
  },
  zh: {
    // Nav
    'nav.translate': '翻译',
    'nav.validate': '校验',
    'nav.visualize': '可视化',
    'nav.history': '历史',
    'nav.functions': '函数库',
    'nav.settings': '设置',

    // Shell / topbar
    'shell.search': '搜索或跳转到…',
    'shell.connected': '已连接',
    'shell.toggleTheme': '切换主题',
    'shell.toggleLang': '切换语言',

    // Translate page
    'translate.title': '翻译',
    'translate.subtitle': '在 Hive、Spark、Trino、GaussDB 方言之间转换 SQL。',
    'translate.newTranslation': '新翻译',
    'translate.from': '源',
    'translate.to': '目标',
    'translate.rewrite': '重写类型',
    'translate.rewriteNone': '无',
    'translate.rewriteIncremental': '增量',
    'translate.placeholder': '在此粘贴 SQL…',
    'translate.hint': '⌘↵ 执行翻译',
    'translate.clear': '清空',
    'translate.format': '格式化',
    'translate.formatFailed': '无法格式化 —— SQL 可能有语法错误？',
    'translate.openRecent': '最近',
    'translate.run': '翻译',
    'translate.running': '翻译中…',
    'translate.chars': '{n} 字符',
    'translate.tokens': '{n} 词元',
    'translate.failed': '翻译失败',
    'translate.success': '成功',
    'translate.copy': '复制',
    'translate.copied': '已复制',
    'translate.translated': '翻译结果',
    'translate.recent': '最近',
    'translate.viewAll': '查看全部',
    'translate.noHistory': '暂无历史。从上方发起一次翻译开始积累。',

    // History page
    'history.title': '历史',
    'history.subtitle': '已保存 {n} 条翻译，仅存在你的浏览器本地。',
    'history.clear': '清空',
    'history.clearConfirm': '确定清空所有翻译历史？',
    'history.filter': '按 SQL、源语言、目标语言过滤…',
    'history.favsOnly': '仅收藏',
    'history.empty': '还没有历史记录。去翻译页面来一条吧。',
    'history.noMatch': '当前过滤条件没有结果。',
    'history.rerun': '重跑',
    'history.delete': '删除',
    'history.source': '源',
    'history.translated': '翻译结果',

    // Functions page
    'functions.title': '函数库',
    'functions.subtitleLoading': '加载函数注册表中…',
    'functions.subtitle': '注册表共 {n} 个函数，支持按条件过滤与搜索。',
    'functions.search': '按名称、说明或分类搜索…',
    'functions.disposition': '处理方式',
    'functions.all': '全部',
    'functions.loading': '加载中…',
    'functions.failed': '加载函数失败：{err}',
    'functions.colName': '名称',
    'functions.colDisposition': '处理',
    'functions.colCategory': '分类',
    'functions.colNotes': '说明',
    'functions.noMatch': '没有匹配项。',

    // Validate page
    'validate.title': '校验',
    'validate.subtitle': '只做 SQL 语法解析，不执行，不需要目录。',
    'validate.parseCheck': '语法检查',
    'validate.hint': '⌘↵ 执行校验',
    'validate.placeholder': '粘贴 SQL…',
    'validate.run': '校验',
    'validate.running': '校验中…',
    'validate.failed': '请求失败',
    'validate.ok': '语法正确',
    'validate.okDetail': 'SQL 在 coral 的 PostgreSQL 解析器下合法。',
    'validate.statementCount': '{n} 条语句',
    'validate.parseError': '解析错误',
    'validate.unknownError': '未知解析错误',

    // Visualize page
    'visualize.title': '可视化',
    'visualize.subtitle':
      '将 SQL 解析树渲染为 Graphviz DOT 或 PlantUML，可粘贴到 graphviz.org 等任意 DOT 渲染器查看。',
    'visualize.ast': '语法树',
    'visualize.format': '格式',
    'visualize.dot': 'Graphviz (DOT)',
    'visualize.plantuml': 'PlantUML',
    'visualize.hint': '⌘↵ 执行渲染',
    'visualize.run': '渲染',
    'visualize.running': '渲染中…',
    'visualize.failed': '渲染失败',
    'visualize.graphSource': '图形源码',
    'visualize.preview': '预览',
    'visualize.previewIn': '渲染器',
    'visualize.openInNewTab': '在新标签页打开',
    'visualize.copy': '复制',
    'visualize.copied': '已复制',
    'visualize.download': '下载',
    'visualize.tipBefore': '提示：粘贴到',
    'visualize.tipOr': '或',
    'visualize.tipAfter': '即可查看。',

    // Settings page
    'settings.title': '设置',
    'settings.subtitle': '主题、语言、后端信息、以及本地目录。',
    'settings.tabGeneral': '常规',
    'settings.tabCatalog': '目录（本地）',
    'settings.tabAbout': '关于',
    'settings.theme': '主题',
    'settings.themeDesc': '选择浅色或深色界面。',
    'settings.light': '浅色',
    'settings.dark': '深色',
    'settings.language': '语言',
    'settings.languageDesc': '选择界面语言。',
    'settings.backend': '后端',
    'settings.backendDesc': 'API 基础地址',
    'settings.backendUnset':
      '（同源访问 — 未设置 NEXT_PUBLIC_CORAL_SERVICE_API_URL）',
    'settings.catalogTitle': '创建数据库 / 表 / 视图',
    'settings.catalogLocalOnly': '仅本地模式',
    'settings.catalogPlaceholder': 'CREATE DATABASE / TABLE / VIEW …',
    'settings.catalogHint':
      'Rust 服务会兼容接收 CREATE 语句；当前翻译不依赖目录持久化。',
    'settings.execute': '执行',
    'settings.executing': '执行中…',
    'settings.catalogFailed': '失败',
    'settings.catalogSuccess': '创建成功',
    'settings.response': '响应',
    'settings.aboutSub': 'coral-rust 工作区 · v0.3',
    'settings.aboutDesc':
      'Coral 是 SQL 翻译、校验与可视化工具链。此前端对接 Rust 后端（coral-service），支持 Hive、Spark、Trino、GaussDB 方言。所有翻译记录仅保存在你的浏览器本地，不经远端服务除非你配置了远程后端。',
    'settings.statFn': '函数注册表',
    'settings.statParser': '解析引擎',
    'settings.statPretty': '美化打印',
    'settings.statTargets': '目标方言',
    'settings.statFnV': '331 个条目',
    'settings.statParserV': 'sqlparser-rs / PostgreSQL',
    'settings.statPrettyV': 'Calcite 风格',
    'settings.statTargetsV': 'Spark · Trino',

    // Command palette
    'cmd.placeholder': '输入命令或搜索历史…',
    'cmd.empty': '没有结果。',
    'cmd.navigate': '跳转',
    'cmd.recent': '最近翻译',
    'cmd.actions': '动作',
    'cmd.toggleTheme': '切换主题',
    'cmd.toggleLang': '切换语言',
    'cmd.clearHistory': '清空历史',
  },
};

const I18nContext = createContext({
  lang: 'en',
  setLang: () => {},
  t: (k) => k,
});

function detectInitialLang() {
  if (typeof window === 'undefined') return 'en';
  const saved = localStorage.getItem('coral.lang');
  if (saved === 'en' || saved === 'zh') return saved;
  const nav = (navigator.language || 'en').toLowerCase();
  return nav.startsWith('zh') ? 'zh' : 'en';
}

export function I18nProvider({ children }) {
  // Render on server with 'en' so SSR output matches initial client markup;
  // swap in the saved/browser preference after hydration.
  const [lang, setLangState] = useState('en');

  useEffect(() => {
    const initial = detectInitialLang();
    if (initial !== 'en') setLangState(initial);
  }, []);

  const setLang = useCallback((next) => {
    setLangState(next);
    if (typeof window !== 'undefined') {
      localStorage.setItem('coral.lang', next);
      document.documentElement.lang = next === 'zh' ? 'zh-CN' : 'en';
    }
  }, []);

  useEffect(() => {
    document.documentElement.lang = lang === 'zh' ? 'zh-CN' : 'en';
  }, [lang]);

  const t = useCallback(
    (key, vars) => {
      const str = (DICT[lang] && DICT[lang][key]) || DICT.en[key] || key;
      if (!vars) return str;
      return str.replace(/\{(\w+)\}/g, (_, k) =>
        vars[k] === undefined ? `{${k}}` : String(vars[k]),
      );
    },
    [lang],
  );

  const value = useMemo(() => ({ lang, setLang, t }), [lang, setLang, t]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  return useContext(I18nContext);
}

export function useT() {
  return useContext(I18nContext).t;
}
