# Manga Manager App - Architecture Design

## Overview

A macOS desktop application built with **Tauri** + **Material Web Components (MWC)** for centralized manga management, primarily using CB7 format, with scraping capabilities and local distribution via OPDS/RSS.

---

## Tech Stack

### Frontend
- **Framework**: Tauri 2.x (Rust backend + WebView frontend)
- **UI Components**: Material Web Components (MWC) - Google's official web components
- **Language**: TypeScript
- **Build Tool**: Vite
- **Styling**: CSS Modules + Material Design tokens

### Backend (Rust)
- **Framework**: Tauri Core
- **Database**: SQLite (via `rusqlite` or `sea-orm`)
- **Web Server**: `axum` or `warp` for OPDS/RSS server
- **Archive**: `cab` crate for CB7 format handling
- **Async Runtime**: `tokio`

### Embedded Scripting
- **Browser**: Tauri WebView (WKWebView on macOS)
- **Script Engine**: Custom Tampermonkey-like userscript runner
- **API Injection**: JavaScript bridge for scraping utilities

---

## Folder Structure

```
manga-manager/
├── src-tauri/                    # Rust backend
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs              # Entry point
│   │   ├── lib.rs
│   │   ├── commands/            # Tauri command handlers
│   │   │   ├── mod.rs
│   │   │   ├── book.rs
│   │   │   ├── scraper.rs
│   │   │   ├── search.rs
│   │   │   └── server.rs
│   │   ├── models/              # Data models
│   │   │   ├── mod.rs
│   │   │   ├── book.rs
│   │   │   ├── scraper.rs
│   │   │   └── job.rs
│   │   ├── services/            # Business logic
│   │   │   ├── mod.rs
│   │   │   ├── library.rs       # Book management
│   │   │   ├── indexer.rs       # Search indexing
│   │   │   ├── scraper.rs       # Scraper orchestration
│   │   │   ├── plugin.rs        # Plugin system
│   │   │   ├── opds.rs          # OPDS feed generation
│   │   │   ├── rss.rs           # RSS feed generation
│   │   │   └── storage.rs       # CB7 storage handling
│   │   ├── plugins/             # Built-in scraper plugins
│   │   │   ├── mod.rs
│   │   │   ├── base.rs          # Plugin trait definition
│   │   │   └── examples/        # Example scrapers
│   │   ├── db/                  # Database layer
│   │   │   ├── mod.rs
│   │   │   ├── connection.rs
│   │   │   ├── migrations/
│   │   │   └── schema.rs
│   │   ├── scripting/           # Userscript engine
│   │   │   ├── mod.rs
│   │   │   ├── engine.rs        # JS runtime setup
│   │   │   ├── bindings.rs      # Exposed API bindings
│   │   │   └── scripts/         # Built-in helper scripts
│   │   └── utils/
│   ├── tauri.conf.json
│   └── assets/
│
├── src/                          # Frontend (TypeScript + MWC)
│   ├── main.tsx
│   ├── App.tsx
│   ├── components/              # React/Custom Elements wrappers
│   │   ├── mwc/                 # MWC component wrappers
│   │   ├── Library/
│   │   ├── Scraper/
│   │   ├── Search/
│   │   └── Browser/
│   ├── views/
│   │   ├── Library.tsx          # Main library view
│   │   ├── Reader.tsx           # CB7 reader view
│   │   ├── Scrapers.tsx         # Scraper management
│   │   ├── Plugins.tsx          # Plugin management
│   │   ├── Browser.tsx          # Embedded browser
│   │   └── Settings.tsx
│   ├── services/
│   │   ├── api.ts               # Tauri command wrappers
│   │   ├── store.ts             # State management
│   │   └── opds-client.ts       # OPDS parser
│   ├── styles/
│   └── types/
│
├── userscripts/                  # User scripts for scraping
│   ├── README.md
│   ├── examples/
│   └── api.d.ts                  # TypeScript definitions for API
│
├── storage/                      # Local storage (symlink/configurable)
│   ├── library/                  # CB7 files
│   ├── metadata/                 # Sidecar metadata
│   ├── cache/                    # Temporary cache
│   └── plugins/                  # User plugin scripts
│
├── docs/
│   ├── ARCHITECTURE.md
│   ├── PLUGIN-API.md
│   └── SCRIPTING-API.md
│
└── tests/
```

---

## Core Data Models

### Book
```typescript
interface Book {
  id: string;                    // UUID
  title: string;
  originalFilename?: string;      // Original filename before import
  filePath: string;              // Path to CB7 file
  fileSize: number;
  format: 'cb7' | 'cbz' | 'cbr' | 'pdf';
  
  // Content metadata
  pageCount: number;
  coverPath?: string;            // Extracted cover image
  
  // Source metadata
  source?: {
    plugin: string;              // Plugin name
    sourceUrl: string;           // Original URL
    scrapedAt: Date;             // When scraped
  };
  
  // Classification
  tags: Tag[];
  collections: Collection[];
  
  // Timestamps
  createdAt: Date;
  updatedAt: Date;
  lastReadAt?: Date;
  readCount: number;
}

interface Tag {
  id: string;
  name: string;
  type: 'genre' | 'artist' | 'author' | 'series' | 'custom';
  value: string;
}

interface Collection {
  id: string;
  name: string;
  description?: string;
}
```

### Scraper Plugin
```typescript
interface ScraperPlugin {
  id: string;
  name: string;
  version: string;
  author: string;
  description: string;
  
  // Capabilities
  capabilities: {
    search: boolean;             // Can search for content
    browse: boolean;             // Can browse categories/lists
    download: boolean;           // Can download full content
    metadata: boolean;           // Can fetch metadata only
  };
  
  // Configuration schema
  configSchema: ConfigSchema;
  
  // Execution
  schedule?: ScraperSchedule;
  
  // Source configuration
  source: {
    baseUrl: string;
    encoding?: string;
    rateLimit?: number;          // Requests per second
  };
}

interface ScraperSchedule {
  enabled: boolean;
  frequency: 'manual' | 'daily' | 'weekly' | 'custom';
  cronExpression?: string;
  
  // Filters
  filters: {
    dateRange?: {
      from: Date;
      to: Date;
    };
    keywords: string[];          // Include keywords
    excludeKeywords: string[];   // Exclude keywords
    tags?: string[];
    maxItems?: number;
  };
}

interface ScraperJob {
  id: string;
  pluginId: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  startedAt?: Date;
  completedAt?: Date;
  itemsProcessed: number;
  itemsFailed: number;
  errors: ErrorEntry[];
}

interface ErrorEntry {
  item: string;
  message: string;
  timestamp: Date;
}
```

---

## Module 1: Book Management (Core)

### Responsibilities
- Import books from files or scrapers into CB7 format
- Extract and store metadata
- Maintain cover images
- Handle file operations (move, delete, export)

### Key Operations
```rust
// Tauri Commands
#[tauri::command]
async fn import_book(file_path: String) -> Result<Book, Error>

#[tauri::command]
async fn import_book_from_images(
    images: Vec<String>,
    metadata: BookMetadata
) -> Result<Book, Error>

#[tauri::command]
async fn delete_book(id: String) -> Result<(), Error>

#[tauri::command]
async fn update_book_metadata(
    id: String,
    metadata: BookMetadata
) -> Result<Book, Error>

#[tauri::command]
async fn get_book_cover(id: String) -> Result<String, Error>  // Returns base64 or path

#[tauri::command]
async fn export_book(
    id: String,
    format: 'cb7' | 'pdf'
) -> Result<String, Error>
```

### CB7 Storage Strategy
- Store CB7 files in `storage/library/` with UUID names
- Extract covers to `storage/covers/` for quick access
- Use SQLite for metadata and search indexing
- Implement lazy loading for large libraries

---

## Module 2: OPDS/RSS Server

### Architecture
```rust
// Server runs on configurable port (default: 8080)
// Provides three endpoints:

GET /opds                       // OPDS catalog root
GET /opds/search/{query}        // Search endpoint
GET /opds/feed/{collection}     // Collection-specific feed

GET /rss                        // RSS feed of all books
GET /rss/{collection}          // Collection-specific RSS
```

### OPDS Feed Structure
```xml
<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:opds="http://opds-spec.org/2010/catalog">
  <id>urn:uuid:...</id>
  <title>Manga Manager Library</title>
  <updated>2026-07-08T...</updated>
  
  <!-- Navigation feeds -->
  <link rel="self" href="/opds"/>
  <link rel="start" href="/opds"/>
  
  <!-- Acquisition feeds -->
  <entry>
    <title>[Book Title]</title>
    <id>urn:uuid:...</id>
    <updated>...</updated>
    <link rel="http://opds-spec.org/acquisition" 
          href="/download/{id}" 
          type="application/x-cb7"/>
    <link rel="thumbnail" href="/covers/{id}.jpg"/>
    <author>
      <name>[Author Name]</name>
    </author>
    <category scheme="..." term="..."/>
    <summary>[Description]</summary>
  </entry>
</feed>
```

### Download Endpoint
```rust
#[tauri::command]
async fn start_opds_server(port: u16) -> Result<String, Error>  // Returns base URL

// HTTP handler
async fn download_book(Path(id): Path<String>) -> impl IntoResponse
```

---

## Module 3: Search & Indexing

### Search Capabilities
- Full-text search over titles, filenames
- Tag filtering (AND/OR combinations)
- Date range filtering
- Source/plugin filtering
- Collection filtering

### Implementation (SQLite FTS5)
```sql
CREATE VIRTUAL TABLE books_fts USING fts5(
    title, 
    original_filename,
    tags,
    content='books',
    content_rowid='id'
);

-- Triggers to keep FTS in sync
CREATE TRIGGER books_ai AFTER INSERT ON books BEGIN
    INSERT INTO books_fts(rowid, title, original_filename, tags)
    VALUES (new.id, new.title, new.original_filename, new.tags);
END;
```

### Rust Search Service
```rust
pub struct SearchService {
    db: Arc<Mutex<Connection>>,
}

impl SearchService {
    pub async fn search(&self, query: SearchQuery) -> Result<Vec<Book>> {
        // Build FTS query from filters
        // Execute with pagination
        // Return ordered results
    }
    
    pub async fn get_tags(&self, type: Option<String>) -> Result<Vec<Tag>> {
        // Return all tags, optionally filtered by type
    }
    
    pub async fn get_collections(&self) -> Result<Vec<Collection>> {
        // Return all collections with counts
    }
}
```

### Frontend Search API
```typescript
interface SearchQuery {
  text?: string;                 // Full-text search
  tags?: string[];               // Tag filter (AND)
  tagsAny?: string[];           // Tag filter (OR)
  collections?: string[];        // Collection filter
  dateRange?: {
    from: Date;
    to: Date;
  };
  sources?: string[];           // Plugin source filter
  sortBy: 'relevance' | 'title' | 'date' | 'size';
  sortOrder: 'asc' | 'desc';
  page: number;
  pageSize: number;
}

interface SearchResult {
  books: Book[];
  total: number;
  facets: {
    tags: Tag[];
    collections: Collection[];
    sources: string[];
  };
}
```

---

## Module 4: Scraper Plugin System

### Plugin Architecture

#### Rust Side (Plugin Host)
```rust
// Plugin trait definition
#[async_trait]
pub trait ScraperPlugin: Send + Sync {
    fn id(&self) -> String;
    fn name(&self) -> String;
    fn version(&self) -> String;
    
    async fn search(&self, query: &str) -> Result<Vec<ScraperResult>>;
    async fn browse(&self, url: &str) -> Result<Vec<ScraperResult>>;
    async fn download(&self, url: &str) -> Result<DownloadResult>;
    
    async fn get_metadata(&self, url: &str) -> Result<BookMetadata>;
    
    // Configuration
    fn default_config(&self) -> serde_json::Value;
    fn validate_config(&self, config: &serde_json::Value) -> Result<()>;
}

pub struct ScraperResult {
    pub title: String,
    pub url: String,
    pub metadata: Option<BookMetadata>,
    pub thumbnail_url: Option<String>,
}

pub struct DownloadResult {
    pub images: Vec<Vec<u8>>,      // Downloaded images
    pub metadata: BookMetadata,
}

// Built-in plugins
mod scrapers {
    pub mod generic_html;    // Generic HTML scraper
    pub mod generic_json;    // Generic JSON API scraper
    // Add more specific scrapers as needed
}
```

#### Plugin Configuration (JSON)
```json
{
  "id": "example-manga-site",
  "type": "generic-html",
  "name": "Example Manga Site",
  "config": {
    "baseUrl": "https://example.com",
    "searchPath": "/search?q={query}",
    "searchResults": {
      "container": ".manga-list .item",
      "title": ".title",
      "url": "a@href",
      "thumbnail": "img@src"
    },
    "chapterList": {
      "container": ".chapter-list .chapter",
      "title": ".chapter-title",
      "url": "a@href"
    },
    "imageList": {
      "container": ".reader-images img",
      "url": "@src"
    },
    "rateLimit": 1000
  },
  "schedule": {
    "enabled": true,
    "frequency": "daily",
    "filters": {
      "keywords": ["action", "fantasy"],
      "excludeKeywords": ["mature"],
      "dateRange": {
        "from": "2024-01-01",
        "to": "2024-12-31"
      }
    }
  }
}
```

### User Plugins Directory
```
storage/plugins/
├── my-custom-scraper.json
├── another-site.json
└── README.md
```

### Scraper Orchestration
```rust
pub struct ScraperOrchestrator {
    plugins: HashMap<String, Box<dyn ScraperPlugin>>,
    job_queue: Arc<Mutex<VecDeque<ScraperJob>>>,
}

impl ScraperOrchestrator {
    pub async fn register_plugin(&mut self, plugin: Box<dyn ScraperPlugin>) {
        self.plugins.insert(plugin.id(), plugin);
    }
    
    pub async fn run_schedule(&self) {
        for plugin in self.plugins.values() {
            if let Some(schedule) = plugin.get_schedule() {
                if self.should_run(schedule) {
                    self.run_plugin(plugin.id()).await;
                }
            }
        }
    }
    
    pub async fn run_plugin(&self, plugin_id: String) -> Result<ScraperJob> {
        let plugin = self.plugins.get(&plugin_id)?;
        
        // 1. Search for content
        let results = plugin.search(&schedule.keywords.join(" ")).await?;
        
        // 2. Filter by date range and keywords
        let filtered = self.filter_results(results, &schedule.filters)?;
        
        // 3. Download each item
        let job = ScraperJob::new(plugin_id);
        for result in filtered {
            let download = plugin.download(&result.url).await?;
            
            // 4. Convert to CB7
            let book = self.convert_to_cb7(download).await?;
            
            // 5. Store in library
            self.library_service.add_book(book).await?;
            
            job.items_processed += 1;
        }
        
        Ok(job)
    }
}
```

---

## Module 5: Embedded Browser + Scripting

### WebView Integration
```rust
// Tauri window configuration
{
  "url": "https://example.com",  // User navigates
  "injectScripts": [
    "https://tampermonkey.net/...",
    "userscript-injected.js"     // Our API injection
  ],
  "webPreferences": {
    "javascript": true,
    "plugins": true,
    "webSecurity": false  // For cross-origin scraping
  }
}
```

### Userscript API (JavaScript)
```javascript
// Injected into WebView context
window.__MangaScraperAPI__ = {
  // Lifecycle hooks
  async onNavigate(url) { /* called on page load */ },
  async onElementFound(selector) { /* optional */ },
  
  // Image utilities
  async fetchImage(url) { /* returns ArrayBuffer */ },
  async fetchImages(urls) { /* parallel fetch */ },
  
  // CB7 creation
  async compactPicsToCB7(metadata, imageFiles) {
    // Calls Rust backend to create CB7
  },
  
  // Library access
  async addBookToLibrary(bookData) { /* saves to library */ },
  async checkExists(identifier) { /* check if already in library */ },
  
  // Progress reporting
  async reportProgress(percent, message) { /* updates UI */ },
  
  // Storage
  async saveCache(key, value) { /* persists data */ },
  async loadCache(key) { /* retrieves data */ },
  
  // Download helpers
  async downloadFile(url, filename) { /* saves to temp */ },
  async openTab(url) { /* opens new tab */ },
};
```

### Userscript Template
```javascript
// ==UserScript==
// @name         My Custom Scraper
// @namespace    http://tampermonkey.net/
// @version      1.0
// @description  Scrape manga from example.com
// @author       User
// @match        https://example.com/manga/*
// @grant        none
// ==/UserScript==

(function() {
  'use strict';
  
  const API = window.__MangaScraperAPI__;
  
  // Initialize when page is ready
  async function init() {
    const mangaUrl = window.location.href;
    const title = document.querySelector('h1.title')?.textContent;
    
    API.reportProgress(10, `Found manga: ${title}`);
    
    // Check if already exists
    if (await API.checkExists(mangaUrl)) {
      API.reportProgress(100, 'Already in library');
      return;
    }
    
    // Find all chapter links
    const chapters = Array.from(document.querySelectorAll('.chapter-link'))
      .map(a => ({ title: a.textContent, url: a.href }));
    
    const allImages = [];
    
    for (let i = 0; i < chapters.length; i++) {
      const chapter = chapters[i];
      API.reportProgress(
        10 + (i / chapters.length) * 70,
        `Downloading ${chapter.title}...`
      );
      
      // Navigate and scrape (simplified)
      const images = await scrapeChapter(chapter.url);
      allImages.push(...images);
    }
    
    API.reportProgress(85, 'Creating CB7...');
    
    // Create CB7
    const metadata = {
      title: title,
      source: { url: mangaUrl },
      scrapedAt: new Date().toISOString(),
      chapterCount: chapters.length,
    };
    
    const cb7File = await API.compactPicsToCB7(metadata, allImages);
    
    API.reportProgress(95, 'Adding to library...');
    
    await API.addBookToLibrary({
      title: title,
      filePath: cb7File.path,
      source: {
        plugin: 'userscript',
        sourceUrl: mangaUrl,
        scrapedAt: new Date(),
      },
    });
    
    API.reportProgress(100, 'Done!');
  }
  
  async function scrapeChapter(url) {
    // Implementation depends on site structure
    // Could use fetch or need to navigate
    const response = await fetch(url);
    const html = await response.text();
    const parser = new DOMParser();
    const doc = parser.parseFromString(html, 'text/html');
    
    const images = doc.querySelectorAll('.reader-image');
    const files = [];
    
    for (const img of images) {
      const data = await API.fetchImage(img.src);
      files.push(new File([data], img.src.split('/').pop()));
    }
    
    return files;
  }
  
  // Run when page is ready
  if (document.readyState === 'complete') {
    init();
  } else {
    window.addEventListener('load', init);
  }
})();
```

### Userscript Management UI
- List installed scripts
- Edit script in built-in editor (Monaco Editor or similar)
- Enable/disable scripts
- See script logs and errors
- Auto-detect compatible sites

---

## UI Design (MWC Components)

### Main Navigation (mwc-drawer)
```
┌─────────────────────────────┐
│  ☰  Manga Manager          │
├─────────────────────────────┤
│  📚 Library                │
│  🔍 Search                 │
│  🤖 Scrapers               │
│  🔌 Plugins                │
│  🌐 Browser                │
│  ⚙️ Settings               │
└─────────────────────────────┘
```

### Library View
```tsx
<mwc-tab-bar>
  <mwc-tab label="All Books"></mwc-tab>
  <mwc-tab label="Collections"></mwc-tab>
  <mwc-tab label="Tags"></mwc-tab>
</mwc-tab-bar>

<mwc-linear-progress></mwc-linear-progress>

<div class="book-grid">
  {books.map(book => (
    <mwc-card class="book-card" onClick={() => openBook(book.id)}>
      <img src={book.coverPath} alt={book.title} />
      <div class="book-info">
        <h3>{book.title}</h3>
        <mwc-chip-set>
          {book.tags.slice(0, 3).map(tag => (
            <mwc-chip label={tag.name}></mwc-chip>
          ))}
        </mwc-chip-set>
      </div>
    </mwc-card>
  ))}
</div>
```

### Search Panel
```tsx
<mwc-textfield
  label="Search"
  icon="search"
  outlined
  value={query}
  onInput={handleSearch}
/>

<mwc-select label="Filter by Tags" multiple>
  {tags.map(tag => (
    <mwc-list-item value={tag.id}>{tag.name}</mwc-list-item>
  ))}
</mwc-select>

<mwc-formfield label="Date Range">
  <mwc-switch checked={useDateRange} onChange={toggleDateFilter} />
</mwc-formfield>

<mwc-button label="Apply Filters" raised onClick={applyFilters} />
```

### Scraper Management
```tsx
<mwc-list>
  {plugins.map(plugin => (
    <mwc-list-item>
      <div slot="secondary">
        <strong>{plugin.name}</strong> - {plugin.description}
      </div>
      <mwc-icon-button icon="settings" slot="meta" />
      <mwc-icon-button icon="play_arrow" slot="meta" />
    </mwc-list-item>
  ))}
</mwc-list>

<mwc-dialog heading="Scraper Progress">
  <mwc-linear-progress progress={job.progress} />
  <mwc-list>
    {job.items.map(item => (
      <mwc-list-item>{item.title}</mwc-list-item>
    ))}
  </mwc-list>
  <mwc-button slot="primaryAction" dialogAction="close">Close</mwc-button>
</mwc-dialog>
```

### Browser View
```tsx
<div class="browser-container">
  <mwc-textfield
    label="URL"
    value={currentUrl}
    onInput={handleUrlChange}
  />
  <mwc-button label="Go" raised onClick={navigate} />
  
  <mwc-icon-button icon="refresh" onClick={refresh} />
  <mwc-icon-button icon="script" onClick={openScriptEditor} />
</div>

<div class="webview-wrapper">
  <webview src={currentUrl} />
</div>

<mwc-fab icon="add" extended label="Scrape Page" onClick={runCurrentScript} />
```

---

## Security Considerations

1. **User Scripts**: Run in isolated WebView context, no direct file system access
2. **Plugin Rate Limits**: Enforce per-plugin rate limits to avoid source blocking
3. **OPDS Server**: Bind to localhost only by default, optional network binding
4. **File Paths**: Never expose absolute paths in API responses
5. **CB7 Validation**: Validate archive contents before adding to library
6. **Download Safety**: Scan downloaded images for suspicious content

---

## Performance Considerations

1. **Lazy Loading**: Load library in pages, prefetch covers
2. **Thumbnail Generation**: Generate smaller thumbnails for quick loading
3. **Index Updates**: Defer FTS index updates in batches
4. **Scraper Jobs**: Run in background thread, report progress via events
5. **OPDS Streaming**: Stream OPDS responses for large libraries

---

## Future Enhancements

1. **Reader View**: Built-in CB7 reader with zoom, page navigation
2. **Cloud Sync**: Optional cloud storage backup
3. **Smart Collections**: Auto-collections based on tags/metadata
4. **Duplicate Detection**: Hash-based duplicate detection
5. **Batch Operations**: Batch tag, move, delete operations
6. **Statistics Dashboard**: Reading stats, scraper success rates
