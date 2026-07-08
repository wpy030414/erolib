# Userscript API Reference

This document describes the JavaScript API available to userscripts running in the embedded browser.

## Overview

The Manga Manager app includes an embedded browser with a custom userscript engine similar to Tampermonkey. Users can write scripts to scrape manga from websites that don't fit the generic plugin pattern.

## Global API

The `__MangaScraperAPI__` object is injected into every page loaded in the embedded browser.

```typescript
interface MangaScraperAPI {
  // Lifecycle hooks
  onNavigate(url: string): void | Promise<void>
  onElementFound(selector: string, element: Element): void | Promise<void>
  
  // Image utilities
  fetchImage(url: string, options?: FetchOptions): Promise<ArrayBuffer>
  fetchImages(urls: string[], options?: FetchOptions): Promise<ArrayBuffer[]>
  
  // CB7 creation
  compactPicsToCB7(metadata: BookMetadata, images: File[]): Promise<File>
  
  // Library access
  addBookToLibrary(book: BookInput): Promise<Book>
  checkExists(identifier: string): Promise<boolean>
  getBook(id: string): Promise<Book>
  
  // Progress reporting
  reportProgress(percent: number, message: string): void
  
  // Storage
  saveCache(key: string, value: any): Promise<void>
  loadCache(key: string): Promise<any>
  clearCache(): Promise<void>
  
  // Download helpers
  downloadFile(url: string, filename?: string): Promise<string>
  openTab(url: string): void
  
  // HTTP utilities
  fetch(url: string, options?: RequestInit): Promise<Response>
  
  // DOM helpers
  waitForElement(selector: string, timeout?: number): Promise<Element>
  querySelectorAll(selector: string): NodeListOf<Element>
  
  // Logging
  log(message: string, level?: 'info' | 'warn' | 'error'): void
}
```

## Detailed API Reference

### Lifecycle Hooks

#### `onNavigate(url: string)`

Called when the browser navigates to a new URL. Use this to initialize your scraper.

```javascript
async function onNavigate(url) {
  if (url.includes('example.com/manga')) {
    await startScraping();
  }
}
```

#### `onElementFound(selector: string, element: Element)`

Called when a watched element is found in the DOM (see `watchElements` below).

---

### Image Utilities

#### `fetchImage(url: string, options?: FetchOptions): Promise<ArrayBuffer>`

Fetches a single image with automatic handling of encoding and redirects.

```javascript
const image = await API.fetchImage('https://example.com/image.jpg');
// Returns ArrayBuffer
```

**Options:**
```typescript
interface FetchOptions {
  method?: 'GET' | 'POST'
  headers?: Record<string, string>
  timeout?: number  // milliseconds
  retries?: number
  Referer?: string  // Set referer header
}
```

#### `fetchImages(urls: string[], options?: FetchOptions): Promise<ArrayBuffer[]>`

Fetches multiple images in parallel with automatic rate limiting.

```javascript
const urls = Array.from(document.querySelectorAll('.page img'))
  .map(img => img.src);

const images = await API.fetchImages(urls, {
  Referer: window.location.href
});
```

---

### CB7 Creation

#### `compactPicsToCB7(metadata: BookMetadata, images: File[]): Promise<File>`

Creates a CB7 file from images and metadata.

**Metadata Format:**
```typescript
interface BookMetadata {
  title: string
  author?: string
  artist?: string
  description?: string
  tags?: string[]
  source?: {
    url: string
    plugin: string
  }
  series?: string
  volume?: number
  chapter?: number
  scannedAt?: string
}
```

**Example:**
```javascript
const images = await API.fetchImages(imageUrls);

const files = images.map((data, index) => {
  return new File([data], `page-${index + 1}.jpg`, {
    type: 'image/jpeg'
  });
});

const cb7File = await API.compactPicsToCB7(
  {
    title: mangaTitle,
    author: authorName,
    tags: genres,
    source: {
      url: window.location.href,
      plugin: 'userscript'
    }
  },
  files
);
```

---

### Library Access

#### `addBookToLibrary(book: BookInput): Promise<Book>`

Adds a book to the library.

```typescript
interface BookInput {
  title: string
  filePath: string  // Path from compactPicsToCB7
  source?: {
    plugin: string
    sourceUrl: string
    scrapedAt: Date
  }
  tags?: string[]
  metadata?: BookMetadata
}
```

**Example:**
```javascript
await API.addBookToLibrary({
  title: mangaTitle,
  filePath: cb7File.name,
  source: {
    plugin: 'userscript',
    sourceUrl: window.location.href,
    scrapedAt: new Date()
  },
  tags: ['action', 'fantasy']
});
```

#### `checkExists(identifier: string): Promise<boolean>`

Checks if a book already exists in the library by URL or title.

```javascript
if (await API.checkExists(window.location.href)) {
  API.reportProgress(100, 'Already in library, skipping...');
  return;
}
```

---

### Progress Reporting

#### `reportProgress(percent: number, message: string)`

Updates the progress indicator in the app UI.

```javascript
API.reportProgress(0, 'Starting...');
API.reportProgress(25, 'Found manga page...');
API.reportProgress(50, 'Downloading images...');
API.reportProgress(100, 'Complete!');
```

---

### Storage

#### `saveCache(key: string, value: any): Promise<void>`

Persists data across page loads and sessions.

```javascript
await API.saveCache('manga-chapters', chapters);
```

#### `loadCache(key: string): Promise<any>`

Retrieves cached data.

```javascript
const cached = await API.loadCache('manga-chapters');
```

---

### Download Helpers

#### `downloadFile(url: string, filename?: string): Promise<string>`

Downloads a file to the temporary directory and returns the path.

```javascript
const path = await API.downloadFile('https://example.com/file.zip', 'archive.zip');
```

#### `openTab(url: string): void`

Opens a new browser tab.

```javascript
API.openTab('https://example.com/manga/chapter-2');
```

---

### DOM Helpers

#### `waitForElement(selector: string, timeout?: number): Promise<Element>`

Waits for an element to appear in the DOM.

```javascript
const loader = await API.waitForElement('.chapter-loader');
```

#### `querySelectorAll(selector: string): NodeListOf<Element>`

Alias for `document.querySelectorAll` with automatic retry.

---

### Logging

#### `log(message: string, level?: 'info' | 'warn' | 'error'): void`

Logs to the app's script console.

```javascript
API.log('Starting scrape...', 'info');
API.log('Failed to download image', 'error');
```

---

## Complete Example

Here's a complete userscript for a hypothetical manga site:

```javascript
// ==UserScript==
// @name         Example Manga Scraper
// @namespace    http://tampermonkey.net/
// @version      1.0
// @description  Scrape manga from example.com
// @author       YourName
// @match        https://example.com/manga/*
// @match        https://example.com/chapter/*
// @grant        none
// ==/UserScript==

(function() {
  'use strict';
  
  const API = window.__MangaScraperAPI__;
  
  // Configuration
  const CONFIG = {
    maxConcurrentDownloads: 5,
    imageTimeout: 10000,
    coverQuality: 'high'
  };
  
  // Scrape manga info page
  async function scrapeMangaPage() {
    API.log('Scraping manga page...', 'info');
    API.reportProgress(10, 'Extracting manga info...');
    
    const title = document.querySelector('h1.title')?.textContent?.trim();
    if (!title) {
      throw new Error('Could not find manga title');
    }
    
    // Check if already exists
    const identifier = `example:${window.location.pathname}`;
    if (await API.checkExists(identifier)) {
      API.reportProgress(100, 'Already in library');
      API.log('Manga already in library, skipping', 'info');
      return;
    }
    
    // Extract metadata
    const metadata = {
      title: title,
      author: document.querySelector('.author')?.textContent?.trim(),
      artist: document.querySelector('.artist')?.textContent?.trim(),
      description: document.querySelector('.summary')?.textContent?.trim(),
      tags: Array.from(document.querySelectorAll('.tag'))
        .map(el => el.textContent.trim()),
      source: {
        url: window.location.href,
        plugin: 'userscript-example'
      }
    };
    
    API.reportProgress(20, 'Finding chapters...');
    
    // Get chapter list
    const chapters = Array.from(document.querySelectorAll('.chapter-list .chapter'))
      .map(el => ({
        title: el.querySelector('.chapter-title')?.textContent?.trim(),
        url: el.querySelector('a')?.href
      }))
      .filter(ch => ch.url);
    
    API.log(`Found ${chapters.length} chapters`, 'info');
    
    // Download each chapter
    const allImages = [];
    for (let i = 0; i < chapters.length; i++) {
      const chapter = chapters[i];
      API.reportProgress(
        20 + (i / chapters.length) * 60,
        `Downloading ${chapter.title} (${i + 1}/${chapters.length})...`
      );
      
      const images = await scrapeChapter(chapter.url);
      allImages.push(...images);
      
      // Small delay between chapters
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
    
    API.reportProgress(85, 'Creating CB7...');
    
    // Create CB7
    const files = allImages.map((data, index) => 
      new File([data], `page-${String(index + 1).padStart(4, '0')}.jpg`, {
        type: 'image/jpeg'
      })
    );
    
    const cb7File = await API.compactPicsToCB7(metadata, files);
    
    API.reportProgress(95, 'Adding to library...');
    
    await API.addBookToLibrary({
      title: metadata.title,
      filePath: cb7File.name,
      source: metadata.source,
      tags: metadata.tags
    });
    
    API.reportProgress(100, 'Done!');
    API.log('Successfully added to library', 'info');
  }
  
  // Scrape individual chapter
  async function scrapeChapter(url) {
    API.log(`Scraping chapter: ${url}`, 'info');
    
    // Navigate to chapter page
    API.openTab(url);
    
    // Wait for images to load
    await API.waitForElement('.reader-images img');
    
    // Get image URLs
    const imageUrls = Array.from(document.querySelectorAll('.reader-images img'))
      .map(img => img.src)
      .filter(src => src);
    
    API.log(`Found ${imageUrls.length} images`, 'info');
    
    // Download images
    const images = await API.fetchImages(imageUrls, {
      Referer: url,
      timeout: CONFIG.imageTimeout
    });
    
    return images;
  }
  
  // Initialize based on page type
  async function init() {
    const url = window.location.href;
    
    if (url.includes('/manga/')) {
      // Manga info page
      await scrapeMangaPage();
    } else if (url.includes('/chapter/')) {
      // Chapter page - this is handled by scrapeChapter
      API.log('On chapter page, waiting for parent page control...', 'info');
    }
  }
  
  // Run when page is ready
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', init);
  } else {
    init();
  }
})();
```

## Advanced Patterns

### Retry Logic

```javascript
async function fetchWithRetry(url, maxRetries = 3) {
  for (let i = 0; i < maxRetries; i++) {
    try {
      return await API.fetchImage(url);
    } catch (error) {
      API.log(`Attempt ${i + 1} failed: ${error.message}`, 'warn');
      if (i === maxRetries - 1) throw error;
      await new Promise(resolve => setTimeout(resolve, 2000 * (i + 1)));
    }
  }
}
```

### Batch Processing

```javascript
async function processInBatches(items, batchSize, processor) {
  for (let i = 0; i < items.length; i += batchSize) {
    const batch = items.slice(i, i + batchSize);
    await Promise.all(batch.map(processor));
  }
}
```

### Cache Management

```javascript
async function getCachedOrFetch(key, fetcher) {
  let data = await API.loadCache(key);
  if (!data) {
    data = await fetcher();
    await API.saveCache(key, data);
  }
  return data;
}
```

### URL Normalization

```javascript
function normalizeUrl(url) {
  return new URL(url, window.location.origin).href;
}
```

## Security Notes

1. **No Direct File Access**: Scripts cannot access the filesystem directly
2. **Same-Origin Policy**: Cross-origin requests use the app's proxy
3. **Rate Limiting**: Built-in rate limits prevent overwhelming servers
4. **Sandbox**: Scripts run in isolated context
5. **No Eval**: Dynamic code execution is blocked

## Debugging

The app provides:
- Script console for log output
- Network inspector for HTTP requests
- Variable inspector for current state
- Step-through debugging support

## Testing Scripts

Use the built-in script tester to:
- Test selectors on live pages
- Inspect extracted data
- Verify image downloads
- Preview CB7 creation
