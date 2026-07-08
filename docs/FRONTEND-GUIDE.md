# Frontend Implementation Guide

This document describes the frontend implementation using TypeScript, React, and Material Web Components (MWC).

## Tech Stack

- **Framework**: React 18+ with TypeScript
- **Build Tool**: Vite
- **UI Components**: Material Web Components (MWC)
- **State Management**: Zustand or React Context
- **Routing**: React Router DOM
- **HTTP Client**: Tauri Invoke API

## Project Setup

```bash
# Create Vite + React + TypeScript project
npm create vite@latest src -- --template react-ts

# Install dependencies
cd src
npm install
npm install @material/mwc-*
npm install react-router-dom zustand
npm install -D @types/react @types/react-dom
```

## Folder Structure

```
src/
├── main.tsx              # Entry point
├── App.tsx               # Root component
├── components/
│   ├── mwc/              # MWC React wrappers
│   │   ├── Button.tsx
│   │   ├── Card.tsx
│   │   ├── Dialog.tsx
│   │   ├── Drawer.tsx
│   │   ├── TextField.tsx
│   │   └── ...
│   ├── Library/          # Library view components
│   │   ├── BookCard.tsx
│   │   ├── BookGrid.tsx
│   │   └── FilterPanel.tsx
│   ├── Search/
│   │   ├── SearchBar.tsx
│   │   ├── SearchResults.tsx
│   │   └── Filters.tsx
│   ├── Scraper/
│   │   ├── PluginList.tsx
│   │   ├── PluginConfig.tsx
│   │   └── JobProgress.tsx
│   └── Browser/
│       ├── BrowserView.tsx
│       └── ScriptEditor.tsx
├── views/
│   ├── Library.tsx
│   ├── Reader.tsx
│   ├── Scrapers.tsx
│   ├── Plugins.tsx
│   ├── Browser.tsx
│   └── Settings.tsx
├── services/
│   ├── api.ts            # Tauri command wrappers
│   ├── store.ts          # Zustand stores
│   └── opds-client.ts    # OPDS feed parser
├── hooks/
│   ├── useBooks.ts
│   ├── useSearch.ts
│   ├── useScrapers.ts
│   └── useBrowser.ts
├── types/
│   ├── index.d.ts        # Shared types
│   └── api.d.ts          # API types
└── styles/
    ├── global.css
    └── variables.css
```

## MWC React Wrappers

Material Web Components are native web components. For better React integration, we create wrapper components:

```tsx
// components/mwc/Button.tsx
import { useEffect, useRef, forwardRef } from 'react';
import '@material/mwc-button';

interface ButtonProps {
  label?: string;
  icon?: string;
  raised?: boolean;
  unelevated?: boolean;
  outlined?: boolean;
  dense?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  children?: React.ReactNode;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ label, icon, raised, unelevated, outlined, dense, disabled, onClick, children }, ref) => {
    const buttonRef = useRef<any>(null);

    useEffect(() => {
      const button = buttonRef.current as any;
      button?.addEventListener('click', onClick);
      return () => button?.removeEventListener('click', onClick);
    }, [onClick]);

    return (
      <mwc-button
        ref={(el) => {
          buttonRef.current = el;
          if (typeof ref === 'function') ref(el);
          else if (ref) ref.current = el;
        }}
        icon={icon}
        raised={raised}
        unelevated={unelevated}
        outlined={outlined}
        dense={dense}
        disabled={disabled}
      >
        {label || children}
      </mwc-button>
    );
  }
);
```

```tsx
// components/mwc/Card.tsx
import '@material/mwc-card';

interface CardProps {
  className?: string;
  onClick?: () => void;
  children: React.ReactNode;
}

export const Card = ({ className, onClick, children }: CardProps) => {
  return (
    <mwc-card className={className} onClick={onClick}>
      {children}
    </mwc-card>
  );
};
```

```tsx
// components/mwc/TextField.tsx
import '@material/mwc-textfield';

interface TextFieldProps {
  label?: string;
  icon?: string;
  outlined?: boolean;
  value?: string;
  type?: string;
  placeholder?: string;
  helper?: string;
  required?: boolean;
  disabled?: boolean;
  onChange?: (value: string) => void;
}

export const TextField = ({
  label,
  icon,
  outlined,
  value,
  type = 'text',
  placeholder,
  helper,
  required,
  disabled,
  onChange,
}: TextFieldProps) => {
  return (
    <mwc-textfield
      label={label}
      icon={icon}
      outlined={outlined}
      value={value}
      type={type}
      placeholder={placeholder}
      helper={helper}
      required={required}
      disabled={disabled}
      onInput={(e: any) => onChange?.(e.target.value)}
    />
  );
};
```

## Type Definitions

```typescript
// types/index.d.ts
export interface Book {
  id: string;
  title: string;
  originalFilename?: string;
  filePath: string;
  fileSize: number;
  format: 'cb7' | 'cbz' | 'cbr' | 'pdf';
  pageCount: number;
  coverPath?: string;
  source?: {
    plugin: string;
    sourceUrl: string;
    scrapedAt: string;
  };
  tags: Tag[];
  collections: Collection[];
  createdAt: string;
  updatedAt: string;
  lastReadAt?: string;
  readCount: number;
}

export interface Tag {
  id: string;
  name: string;
  type: 'genre' | 'artist' | 'author' | 'series' | 'custom';
}

export interface Collection {
  id: string;
  name: string;
  description?: string;
}

export interface ScraperPlugin {
  id: string;
  name: string;
  version: string;
  author?: string;
  description: string;
  type: 'generic-html' | 'generic-json' | 'custom';
  enabled: boolean;
  schedule?: ScraperSchedule;
}

export interface ScraperSchedule {
  enabled: boolean;
  frequency: 'manual' | 'hourly' | 'daily' | 'weekly' | 'custom';
  cronExpression?: string;
  filters: {
    dateRange?: { from: string; to: string };
    keywords: string[];
    excludeKeywords: string[];
    tags?: string[];
    maxItems?: number;
  };
}

export interface ScraperJob {
  id: string;
  pluginId: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  startedAt?: string;
  completedAt?: string;
  itemsProcessed: number;
  itemsFailed: number;
  errors: ErrorEntry[];
}

export interface SearchQuery {
  text?: string;
  tags?: string[];
  tagsAny?: string[];
  collections?: string[];
  dateRange?: { from: Date; to: Date };
  sources?: string[];
  sortBy: 'relevance' | 'title' | 'date' | 'size';
  sortOrder: 'asc' | 'desc';
  page: number;
  pageSize: number;
}

export interface SearchResult {
  books: Book[];
  total: number;
  facets: {
    tags: Tag[];
    collections: Collection[];
    sources: string[];
  };
}
```

## API Service Layer

```typescript
// services/api.ts
import { invoke } from '@tauri-apps/api/tauri';
import type { Book, SearchResult, ScraperPlugin, ScraperJob } from '../types';

export const api = {
  // Book operations
  importBook: (filePath: string) => invoke<Book>('import_book', { filePath }),
  deleteBook: (id: string) => invoke<void>('delete_book', { id }),
  updateBookMetadata: (id: string, metadata: any) =>
    invoke<Book>('update_book_metadata', { id, metadata }),
  getBookCover: (id: string) => invoke<string>('get_book_cover', { id }),
  exportBook: (id: string, format: string) =>
    invoke<string>('export_book', { id, format }),

  // Search
  searchBooks: (query: SearchResult) =>
    invoke<SearchResult>('search_books', { query }),
  getAllTags: () => invoke<Tag[]>('get_all_tags'),
  getAllCollections: () => invoke<Collection[]>('get_all_collections'),

  // Scrapers
  runScraper: (pluginId: string) => invoke<string>('run_scraper', { pluginId }),
  getScraperJobs: () => invoke<ScraperJob[]>('get_scraper_jobs'),
  cancelJob: (jobId: string) => invoke<void>('cancel_job', { jobId }),

  // OPDS Server
  startOpdsServer: (port: number) =>
    invoke<string>('start_opds_server', { port }),
  stopOpdsServer: () => invoke<void>('stop_opds_server'),
};
```

## State Management with Zustand

```typescript
// services/store.ts
import create from 'zustand';
import type { Book, SearchQuery, SearchResult } from '../types';

interface BookStore {
  books: Book[];
  searchQuery: SearchQuery;
  searchResult: SearchResult | null;
  isLoading: boolean;
  error: string | null;

  setBooks: (books: Book[]) => void;
  setSearchQuery: (query: Partial<SearchQuery>) => void;
  search: () => Promise<void>;
  deleteBook: (id: string) => Promise<void>;
}

export const useBookStore = create<BookStore>((set, get) => ({
  books: [],
  searchQuery: {
    sortBy: 'relevance',
    sortOrder: 'desc',
    page: 1,
    pageSize: 50,
  },
  searchResult: null,
  isLoading: false,
  error: null,

  setBooks: (books) => set({ books }),

  setSearchQuery: (query) => set((state) => ({
    searchQuery: { ...state.searchQuery, ...query },
  })),

  search: async () => {
    set({ isLoading: true, error: null });
    try {
      const result = await api.searchBooks(get().searchQuery);
      set({ searchResult: result, books: result.books, isLoading: false });
    } catch (error) {
      set({ error: (error as Error).message, isLoading: false });
    }
  },

  deleteBook: async (id) => {
    await api.deleteBook(id);
    set((state) => ({
      books: state.books.filter((b) => b.id !== id),
    }));
  },
}));
```

## View Components

### Library View

```tsx
// views/Library.tsx
import { useEffect } from 'react';
import { useBookStore } from '../services/store';
import { BookGrid } from '../components/Library/BookGrid';
import { FilterPanel } from '../components/Library/FilterPanel';
import { Button } from '../components/mwc/Button';
import { TextField } from '../components/mwc/TextField';
import { Drawer, DrawerAppContent, DrawerHeader } from '../components/mwc/Drawer';

export function Library() {
  const { books, search, setSearchQuery, isLoading } = useBookStore();

  useEffect(() => {
    search();
  }, []);

  return (
    <div className="library-view">
      <Drawer open>
        <DrawerHeader>
          <h2>Manga Manager</h2>
        </DrawerHeader>

        <FilterPanel />

        <div className="drawer-nav">
          <Button label="Library" icon="book" raised />
          <Button label="Scrapers" icon="smart_toy" />
          <Button label="Browser" icon="language" />
          <Button label="Settings" icon="settings" />
        </div>
      </Drawer>

      <DrawerAppContent>
        <div className="toolbar">
          <TextField
            label="Search"
            icon="search"
            outlined
            placeholder="Search by title, tags..."
            onChange={(value) => setSearchQuery({ text: value })}
          />
          <Button label="Search" raised onClick={search} />
        </div>

        {isLoading ? (
          <mwc-circular-progress indeterminate />
        ) : (
          <BookGrid books={books} />
        )}
      </DrawerAppContent>
    </div>
  );
}
```

### Book Card Component

```tsx
// components/Library/BookCard.tsx
import type { Book } from '../../types';
import { Card } from '../mwc/Card';
import { Chip } from '../mwc/Chip';

interface BookCardProps {
  book: Book;
  onClick: () => void;
}

export function BookCard({ book, onClick }: BookCardProps) {
  return (
    <Card className="book-card" onClick={onClick}>
      {book.coverPath ? (
        <img src={book.coverPath} alt={book.title} className="book-cover" />
      ) : (
        <div className="book-placeholder">
          <span>{book.title.charAt(0)}</span>
        </div>
      )}

      <div className="book-info">
        <h3 className="book-title">{book.title}</h3>
        <p className="book-meta">
          {book.pageCount} pages • {book.format.toUpperCase()}
        </p>

        {book.tags.length > 0 && (
          <div className="book-tags">
            {book.tags.slice(0, 3).map((tag) => (
              <Chip key={tag.id} label={tag.name} />
            ))}
          </div>
        )}
      </div>
    </Card>
  );
}
```

### Browser View

```tsx
// views/Browser.tsx
import { useState, useRef } from 'react';
import { useBrowserStore } from '../services/store';
import { TextField } from '../mwc/TextField';
import { Button } from '../mwc/Button';
import { Fab } from '../mwc/Fab';
import { Dialog } from '../mwc/Dialog';

export function Browser() {
  const [currentUrl, setCurrentUrl] = useState('https://example.com');
  const webviewRef = useRef<any>(null);
  const { runScript, scriptProgress } = useBrowserStore();

  const navigate = () => {
    if (webviewRef.current) {
      webviewRef.current.src = currentUrl;
    }
  };

  return (
    <div className="browser-view">
      <div className="browser-toolbar">
        <TextField
          value={currentUrl}
          onChange={setCurrentUrl}
          placeholder="Enter URL..."
          outlined
        />
        <Button label="Go" raised onClick={navigate} />
        <Button label="Run Script" icon="play_arrow" onClick={runScript} />
      </div>

      <div className="webview-container">
        <webview
          ref={webviewRef}
          src={currentUrl}
          style={{ width: '100%', height: '100%' }}
        />
      </div>

      <Fab
        icon="add"
        label="Scrape"
        extended
        className="scrape-fab"
        onClick={runScript}
      />

      <Dialog open={scriptProgress.active} heading="Scraping Progress">
        <mwc-linear-progress progress={scriptProgress.percent} />
        <p>{scriptProgress.message}</p>
      </Dialog>
    </div>
  );
}
```

## Custom Hooks

```typescript
// hooks/useBooks.ts
import { useEffect } from 'react';
import { useBookStore } from '../services/store';

export function useBooks() {
  const { books, isLoading, error, search } = useBookStore();

  useEffect(() => {
    search();
  }, []);

  return { books, isLoading, error, refresh: search };
}
```

```typescript
// hooks/useScrapers.ts
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import type { ScraperPlugin, ScraperJob } from '../types';

export function useScrapers() {
  const [plugins, setPlugins] = useState<ScraperPlugin[]>([]);
  const [jobs, setJobs] = useState<ScraperJob[]>([]);
  const [isLoading, setIsLoading] = useState(false);

  const loadPlugins = async () => {
    setIsLoading(true);
    try {
      const result = await invoke<ScraperPlugin[]>('get_scraper_plugins');
      setPlugins(result);
    } finally {
      setIsLoading(false);
    }
  };

  const loadJobs = async () => {
    const result = await invoke<ScraperJob[]>('get_scraper_jobs');
    setJobs(result);
  };

  const runPlugin = async (pluginId: string) => {
    await invoke('run_scraper', { pluginId });
    loadJobs();
  };

  useEffect(() => {
    loadPlugins();
    const interval = setInterval(loadJobs, 2000);
    return () => clearInterval(interval);
  }, []);

  return { plugins, jobs, isLoading, runPlugin, loadPlugins };
}
```

## Styling

```css
/* styles/variables.css */
:root {
  --mdc-theme-primary: #6200ee;
  --mdc-theme-secondary: #03dac6;
  --mdc-theme-background: #ffffff;
  --mdc-theme-surface: #f5f5f5;
  --mdc-theme-error: #b00020;

  --app-drawer-width: 280px;
  --app-header-height: 64px;
}

/* styles/global.css */
body {
  margin: 0;
  font-family: 'Roboto', sans-serif;
  -webkit-font-smoothing: antialiased;
}

.library-view {
  display: flex;
  height: 100vh;
}

.book-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 16px;
  padding: 16px;
}

.book-card {
  cursor: pointer;
  transition: transform 0.2s, box-shadow 0.2s;
}

.book-card:hover {
  transform: translateY(-4px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
}

.book-cover {
  width: 100%;
  aspect-ratio: 2/3;
  object-fit: cover;
}

.book-placeholder {
  width: 100%;
  aspect-ratio: 2/3;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--mdc-theme-surface);
  font-size: 48px;
  font-weight: bold;
  color: var(--mdc-theme-primary);
}

.browser-view {
  display: flex;
  flex-direction: column;
  height: 100vh;
}

.browser-toolbar {
  display: flex;
  gap: 8px;
  padding: 8px;
  background: var(--mdc-theme-surface);
}

.webview-container {
  flex: 1;
}

.scrape-fab {
  position: fixed;
  bottom: 16px;
  right: 16px;
}
```

## Main Entry Point

```tsx
// main.tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './styles/global.css';
import './styles/variables.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

```tsx
// App.tsx
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { Library } from './views/Library';
import { Browser } from './views/Browser';
import { Scrapers } from './views/Scrapers';
import { Plugins } from './views/Plugins';
import { Settings } from './views/Settings';

function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<Library />} />
        <Route path="/browser" element={<Browser />} />
        <Route path="/scrapers" element={<Scrapers />} />
        <Route path="/plugins" element={<Plugins />} />
        <Route path="/settings" element={<Settings />} />
      </Routes>
    </BrowserRouter>
  );
}

export default App;
```

## Tauri Configuration

```json
// src-tauri/tauri.conf.json
{
  "build": {
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build",
    "devPath": "http://localhost:5173",
    "distDir": "../dist"
  },
  "package": {
    "productName": "Manga Manager",
    "version": "0.1.0"
  },
  "tauri": {
    "allowlist": {
      "all": false,
      "shell": {
        "all": false,
        "open": true
      },
      "dialog": {
        "all": false,
        "open": true,
        "save": true
      },
      "fs": {
        "all": false,
        "readFile": true,
        "writeFile": true,
        "scope": ["**"]
      }
    },
    "bundle": {
      "active": true,
      "category": "Public.AppCategory.GraphicsDesign",
      "copyright": "",
      "deb": {
        "depends": []
      },
      "externalBin": [],
      "icon": [
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
        "icons/icon.icns",
        "icons/icon.ico"
      ],
      "identifier": "com.mangamanager.app",
      "longDescription": "",
      "macOS": {
        "entitlements": null,
        "exceptionDomain": "",
        "frameworks": [],
        "providerShortName": null,
        "signingIdentity": null
      },
      "resources": [],
      "shortDescription": "",
      "targets": "all",
      "windows": {
        "certificateThumbprint": null,
        "digestAlgorithm": "sha256",
        "timestampUrl": ""
      }
    },
    "security": {
      "csp": null
    },
    "updater": {
      "active": false
    },
    "windows": [{
      "fullscreen": false,
      "height": 900,
      "resizable": true,
      "title": "Manga Manager",
      "width": 1200,
      "minWidth": 800,
      "minHeight": 600,
      "transparent": false
    }]
  }
}
```

## Building and Running

```bash
# Install Tauri CLI
cargo install tauri-cli

# Development
npm run tauri dev

# Build
npm run tauri build

# The build output will be in:
# src-tauri/target/release/bundle/macos/
```
