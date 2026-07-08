# Manga Manager App - Documentation Index

Complete documentation for the Tauri + MWC Manga Manager application.

## Quick Start

### Prerequisites

- macOS 12+ (Monterey or later)
- Xcode Command Line Tools
- Rust 1.70+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Node.js 18+ (`brew install node`)
- npm 9+

### Initial Setup

```bash
# Clone the repository
git clone https://github.com/yourusername/manga-manager.git
cd manga-manager

# Install Rust dependencies
cd src-tauri
cargo build

# Install Node dependencies
cd ../src
npm install

# Run in development mode
npm run tauri dev
```

### First Run

1. **Configure Storage Location**
   - Go to Settings → Storage
   - Set your library path (default: `~/Library/Application Support/Manga Manager`)

2. **Import Your First Book**
   - Click "Import" → "From File"
   - Select a CB7/CBZ/CBR file
   - The book will be added to your library

3. **Set Up OPDS Server** (Optional)
   - Go to Settings → Server
   - Enable "Start OPDS Server"
   - Set port (default: 8080)
   - Access at `http://localhost:8080/opds`

## Documentation

| Document | Description |
|----------|-------------|
| [Architecture Design](../manga-manager-app-design.md) | Overall system architecture, folder structure, and core modules |
| [Plugin API](./PLUGIN-API.md) | How to create scraper plugins (JSON-based configuration) |
| [Scripting API](./SCRIPTING-API.md) | JavaScript API for userscripts in embedded browser |
| [Rust Implementation](./RUST-IMPLEMENTATION.md) | Backend implementation details and database schema |
| [Frontend Guide](./FRONTEND-GUIDE.md) | Frontend implementation with React + MWC |
| [Quick Start](#quick-start) | This document |

## Core Concepts

### Book Storage Format

The app primarily uses **CB7** (Comic Book 7-Zip) format, which is a ZIP archive containing:
- Comic pages as images (JPG, PNG)
- Optional `ComicInfo.xml` metadata file

Why CB7?
- Open format (not tied to any tool)
- Compresses well
- Can store metadata
- Supported by most comic readers

### Library Structure

```
Library Root/
├── library/           # CB7 files (UUID names)
├── covers/            # Extracted cover thumbnails
├── metadata/          # Sidecar metadata
└── cache/             # Temporary downloads
```

### Scraping Workflow

```
User Configures Plugin
    ↓
Schedule Triggers (or Manual Run)
    ↓
Plugin Searches Source
    ↓
Filters Applied (date, keywords, etc.)
    ↓
Chapter Pages Downloaded
    ↓
Images Extracted & Converted
    ↓
CB7 Created
    ↓
Book Added to Library
```

## Common Tasks

### Import Books

**From File:**
```
Library → Import → From File → Select file
```

**From Folder:**
```
Library → Import → From Folder → Select folder
```

**Batch Import:**
```
Library → Import → Batch → Select files/folders
```

### Search Books

**Quick Search:**
- Type in the search bar (searches titles, filenames)

**Advanced Search:**
- Click the filter icon
- Filter by tags, collections, date range, source
- Combine multiple filters

### Configure Scraper

**Built-in Plugin:**
```
Scrapers → Select Plugin → Configure → Test → Enable
```

**Custom Plugin:**
```
Scrapers → Create Plugin → Edit JSON → Save → Test
```

See [Plugin API](./PLUGIN-API.md) for configuration details.

### Use Userscripts

1. Go to Browser tab
2. Navigate to manga site
3. Click "Script Editor"
4. Write or paste script
5. Click "Run"

See [Scripting API](./SCRIPTING-API.md) for API reference.

### Access via OPDS

1. Enable OPDS Server in Settings
2. Add `http://localhost:8080/opds` to your OPDS client
3. Browse your library

Supported clients:
- Chunky (Comic reader for iOS)
- KYBook 2
- Marvin
- Any OPDS-compatible reader

## Plugin Development

### Simple HTML Scraper

Create a file `plugins/my-site.json`:

```json
{
  "id": "my-site",
  "type": "generic-html",
  "name": "My Manga Site",
  "config": {
    "baseUrl": "https://example.com",
    "search": {
      "enabled": true,
      "urlTemplate": "/search?q={query}",
      "results": {
        "container": ".manga-item",
        "title": ".title",
        "url": "a@href"
      }
    },
    "imageList": {
      "container": ".page-image",
      "url": "@src"
    }
  }
}
```

See [Plugin API](./PLUGIN-API.md) for more examples.

## Userscript Example

```javascript
// ==UserScript==
// @name         Simple Scraper
// @match        https://example.com/manga/*
// ==/UserScript==

(async () => {
  const API = window.__MangaScraperAPI__;
  const title = document.querySelector('h1')?.textContent;
  const images = Array.from(document.querySelectorAll('.page img'))
    .map(img => img.src);

  API.reportProgress(0, `Scraping ${title}...`);
  const imageData = await API.fetchImages(images);

  API.reportProgress(80, 'Creating CB7...');
  const files = imageData.map((data, i) =>
    new File([data], `page-${i + 1}.jpg`)
  );

  const cb7 = await API.compactPicsToCB7(
    { title, source: { url: location.href } },
    files
  );

  API.reportProgress(100, 'Done!');
  await API.addBookToLibrary({ title, filePath: cb7.name });
})();
```

See [Scripting API](./SCRIPTING-API.md) for complete API.

## Troubleshooting

### Import Fails

- Check file format is supported (CB7, CBZ, CBR, PDF)
- Ensure file isn't corrupted
- Check disk space

### Scraper Not Working

- Verify site is accessible
- Check selectors match current site HTML
- Review error logs in Scraper → Jobs
- Test with "Test Run" button

### OPDS Not Accessible

- Ensure server is running in Settings
- Check firewall settings
- Verify port isn't in use
- Try `http://127.0.0.1:8080/opds`

### Scripts Not Running

- Check browser console for errors
- Verify API is available (`window.__MangaScraperAPI__`)
- Check script matches URL pattern
- Review script logs

## Development

### Project Structure

```
manga-manager/
├── src/                    # Frontend (TypeScript + React)
├── src-tauri/              # Backend (Rust)
├── userscripts/            # Example userscripts
├── storage/                # Runtime data (symlink)
└── docs/                   # This documentation
```

### Running Tests

```bash
# Rust tests
cd src-tauri
cargo test

# Frontend tests
cd src
npm test
```

### Building

```bash
# Development build
npm run tauri dev

# Production build
npm run tauri build

# Output: src-tauri/target/release/bundle/
```

## Roadmap

### Version 0.1 (MVP)
- ✅ Basic book management
- ✅ CB7 support
- ✅ Search and filtering
- ✅ OPDS server
- ⏳ Basic scraper plugins
- ⏳ Embedded browser

### Version 0.2
- ⏳ Advanced scraper scheduling
- ⏳ Userscript marketplace
- ⏳ Reading statistics
- ⏳ Smart collections

### Version 0.3
- ⏳ Cloud sync
- ⏳ Mobile companion app
- ⏳ Plugin marketplace
- ⏳ Reading recommendations

## Contributing

Contributions welcome! Areas of interest:
- Additional scraper plugins
- UI improvements
- Bug fixes
- Documentation
- Performance optimizations

## License

MIT License - see LICENSE file for details.

## Support

- Issues: https://github.com/yourusername/manga-manager/issues
- Discussions: https://github.com/yourusername/manga-manager/discussions
