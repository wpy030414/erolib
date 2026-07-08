# Scraper Plugin API Reference

This document describes the plugin API for building custom scrapers for the Manga Manager application.

## Overview

Plugins are JSON configuration files that define how to scrape manga from specific websites. The app includes a built-in "generic" scraper that can handle most HTML-based sites using CSS selectors.

## Plugin Structure

```json
{
  "$schema": "../../schemas/plugin-schema.json",
  "id": "unique-plugin-id",
  "type": "generic-html",
  "name": "Display Name",
  "version": "1.0.0",
  "author": "Author Name",
  "description": "Plugin description",
  
  "config": { /* ... */ },
  "schedule": { /* ... */ },
  "metadata": { /* ... */ }
}
```

## Plugin Types

### `generic-html`

For sites that can be scraped using CSS selectors. This is the most common type.

### `generic-json`

For sites that provide JSON APIs.

### `custom`

For advanced use cases requiring custom Rust code (built-in only).

## Generic HTML Plugin Configuration

### Base Configuration

```json
{
  "config": {
    "baseUrl": "https://example.com",
    "encoding": "utf-8",
    "rateLimit": 1000,
    "timeout": 30000,
    "userAgent": "Mozilla/5.0...",
    "headers": {
      "Referer": "https://example.com",
      "Accept-Language": "en-US,en;q=0.9"
    }
  }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `baseUrl` | string | Yes | Base URL for the site |
| `encoding` | string | No | Character encoding (default: utf-8) |
| `rateLimit` | number | No | Delay between requests in ms (default: 1000) |
| `timeout` | number | No | Request timeout in ms (default: 30000) |
| `userAgent` | string | No | Custom User-Agent string |
| `headers` | object | No | Custom HTTP headers |

### Search Configuration

```json
{
  "config": {
    "search": {
      "enabled": true,
      "method": "GET",
      "urlTemplate": "/search?q={query}&page={page}",
      "bodyTemplate": null,
      "results": {
        "container": ".manga-list .item",
        "title": ".title",
        "url": "a@href",
        "thumbnail": ".cover img@src",
        "metadata": {
          "author": ".author",
          "description": ".description",
          "status": ".status",
          "tags": ".tags span"
        },
        "pagination": {
          "next": ".pagination .next@href",
          "totalItems": ".pagination .total",
          "totalPages": ".pagination .pages"
        }
      }
    }
  }
}
```

### Selector Syntax

| Syntax | Description | Example |
|--------|-------------|---------|
| `.class` | CSS class selector | `.title` |
| `#id` | ID selector | `#main-title` |
| `element` | Element selector | `h1` |
| `parent child` | Descendant selector | `.item .title` |
| `parent > child` | Direct child | `.list > .item` |
| `[attr]` | Attribute selector | `[data-id]` |
| `[attr=value]` | Attribute value | `[href="/manga/1"]` |
| `@attr` | Extract attribute | `a@href` |
| `@text` | Extract text content (default) | `.title@text` |
| `@html` | Extract inner HTML | `.content@html` |

### Browse Configuration

```json
{
  "config": {
    "browse": {
      "enabled": true,
      "startUrl": "/manga-list",
      "listConfig": {
        "container": ".manga-item",
        "title": ".title",
        "url": "a@href"
      },
      "pagination": {
        "next": ".next-page@href"
      }
    }
  }
}
```

### Chapter List Configuration

```json
{
  "config": {
    "chapterList": {
      "urlTemplate": "{mangaUrl}/chapters",
      "results": {
        "container": ".chapter-item",
        "title": ".chapter-title",
        "url": "a@href",
        "metadata": {
          "publishedAt": ".date@text",
          "views": ".views@text"
        },
        "sorting": {
          "selector": ".chapter-number",
          "type": "number",
          "order": "desc"
        }
      }
    }
  }
}
```

### Image List Configuration

```json
{
  "config": {
    "imageList": {
      "container": ".reader-images img",
      "url": "@src",
      "headers": {
        "Referer": "{pageUrl}"
      },
      "transformation": {
        "replace": [
          ["http://", "https://"],
          ["_thumb.", ""]
        ]
      },
      "delay": 500,
      "retry": 3
    }
  }
}
```

## Metadata Extraction

### Metadata Mapping

```json
{
  "metadata": {
    "mappings": {
      "title": ".detail-title@text",
      "alternativeTitles": [".alt-title@text"],
      "author": ".author@text",
      "artist": ".artist@text",
      "description": ".summary@text",
      "coverUrl": ".cover img@src",
      "status": {
        "selector": ".status@text",
        "map": {
          "Ongoing": "ongoing",
          "Completed": "completed",
          "Hiatus": "hiatus"
        }
      },
      "tags": [".tag-item@text"],
      "rating": {
        "selector": ".rating@text",
        "transform": "parseFloat"
      },
      "views": {
        "selector": ".views@text",
        "transform": "parseInt"
      }
    },
    "computed": {
      "displayName": "{title} - {author}",
      "searchIndex": "{title} {tags} {description}"
    }
  }
}
```

### Transform Functions

Available transform functions:
- `parseInt` - Parse as integer
- `parseFloat` - Parse as float
- `trim` - Remove whitespace
- `lowercase` - Convert to lowercase
- `uppercase` - Convert to uppercase
- `date` - Parse as date (ISO 8601 output)
- `url` - Resolve relative URL
- `stripHTML` - Remove HTML tags

## Schedule Configuration

```json
{
  "schedule": {
    "enabled": true,
    "frequency": "daily",
    "cronExpression": "0 2 * * *",
    "timezone": "America/New_York",
    
    "searchQueries": [
      "action fantasy",
      "isekai",
      "slice of life"
    ],
    
    "filters": {
      "dateRange": {
        "from": "2024-01-01",
        "to": "2024-12-31"
      },
      "keywords": {
        "include": ["action", "fantasy"],
        "exclude": ["mature", "gore"]
      },
      "tags": ["ongoing"],
      "minRating": 4.0,
      "maxItems": 100,
      "requireCover": true
    },
    
    "downloadOptions": {
      "downloadChapters": true,
      "chapterRange": {
        "from": 1,
        "to": "latest"
      },
      "createSeparateBooks": false,
      "groupBy": "volume",
      "format": "cb7"
    }
  }
}
```

## Schedule Frequencies

| Value | Description |
|-------|-------------|
| `manual` | Only run manually |
| `hourly` | Every hour |
| `daily` | Once per day (default: 2 AM) |
| `weekly` | Once per week (default: Sunday 2 AM) |
| `custom` | Use cron expression |

## Plugin Examples

### Example 1: Simple HTML Scraper

```json
{
  "id": "example-simple",
  "type": "generic-html",
  "name": "Example Simple Site",
  "version": "1.0.0",
  "description": "Simple manga site scraper",
  
  "config": {
    "baseUrl": "https://example.com",
    
    "search": {
      "enabled": true,
      "urlTemplate": "/search?q={query}",
      "results": {
        "container": ".manga-list .item",
        "title": ".title",
        "url": "a@href",
        "thumbnail": ".cover img@src"
      }
    },
    
    "chapterList": {
      "results": {
        "container": ".chapter-list .chapter",
        "title": ".chapter-title",
        "url": "a@href"
      }
    },
    
    "imageList": {
      "container": ".reader-images img",
      "url": "@src"
    }
  },
  
  "metadata": {
    "mappings": {
      "title": "h1.title",
      "author": ".author",
      "description": ".summary"
    }
  }
}
```

### Example 2: Advanced Scraper with Filters

```json
{
  "id": "example-advanced",
  "type": "generic-html",
  "name": "Example Advanced Site",
  "version": "1.0.0",
  
  "config": {
    "baseUrl": "https://example.com",
    "rateLimit": 2000,
    
    "search": {
      "enabled": true,
      "urlTemplate": "/search?q={query}&page={page}",
      "results": {
        "container": ".manga-item",
        "title": ".title",
        "url": "a@href",
        "thumbnail": ".thumb img@src",
        "metadata": {
          "author": ".author",
          "status": ".status",
          "rating": {
            "selector": ".rating@text",
            "transform": "parseFloat"
          }
        },
        "pagination": {
          "next": ".next@href"
        }
      }
    },
    
    "imageList": {
      "container": ".page-image",
      "url": "@src",
      "headers": {
        "Referer": "{pageUrl}"
      },
      "transformation": {
        "replace": [
          ["http://example.com/thumb/", "https://cdn.example.com/full/"]
        ]
      }
    }
  },
  
  "schedule": {
    "enabled": true,
    "frequency": "daily",
    "searchQueries": ["action", "fantasy", "comedy"],
    "filters": {
      "keywords": {
        "include": ["action", "fantasy"],
        "exclude": ["mature"]
      },
      "minRating": 4.0,
      "requireCover": true,
      "maxItems": 50
    }
  },
  
  "metadata": {
    "mappings": {
      "title": "h1.title",
      "alternativeTitles": [".alt-title"],
      "author": ".author@text",
      "artist": ".artist@text",
      "description": ".summary@text",
      "status": {
        "selector": ".status@text",
        "map": {
          "Ongoing": "ongoing",
          "Completed": "completed"
        }
      },
      "tags": [".tag@text"],
      "rating": {
        "selector": ".rating@text",
        "transform": "parseFloat"
      }
    }
  }
}
```

### Example 3: JSON API Scraper

```json
{
  "id": "example-api",
  "type": "generic-json",
  "name": "Example API Site",
  "version": "1.0.0",
  
  "config": {
    "baseUrl": "https://api.example.com/v1",
    "headers": {
      "Authorization": "Bearer {API_TOKEN}",
      "Accept": "application/json"
    }
  },
  
  "search": {
    "enabled": true,
    "method": "GET",
    "urlTemplate": "/manga?search={query}&page={page}",
    "results": {
      "path": "data.results",
      "title": "title",
      "url": "slug",
      "thumbnail": "coverImage",
      "metadata": {
        "author": "author.name",
        "description": "summary"
      }
    }
  },
  
  "chapterList": {
    "path": "chapters",
    "title": "chapterTitle",
    "url": "chapterUrl"
  },
  
  "imageList": {
    "path": "pages",
    "url": "imageUrl"
  }
}
```

## JSON Path Syntax for `generic-json`

| Syntax | Description |
|--------|-------------|
| `field` | Direct field access |
| `field.nested` | Nested field |
| `array[0]` | Array index |
| `array[*]` | All array items |
| `field[*].name` | Map over array |

## Plugin Storage Location

User plugins are stored in:
- **macOS**: `~/Library/Application Support/Manga Manager/plugins/`
- **Linux**: `~/.config/Manga Manager/plugins/`

Built-in plugins are compiled into the application.

## Error Handling

Plugins should define error handling:

```json
{
  "config": {
    "errorHandling": {
      "onNotFound": "skip",
      "onError": "retry",
      "maxRetries": 3,
      "retryDelay": 5000,
      "logErrors": true
    }
  }
}
```

## Validation

The app validates plugins on load:
1. JSON schema validation
2. URL accessibility check
3. Selector test (for HTML plugins)
4. Required fields check

Invalid plugins are disabled with error messages in the UI.

## Plugin Development Workflow

1. Create plugin JSON file
2. Test selectors using built-in tester
3. Run dry-run scrape to verify results
4. Enable schedule if needed
5. Monitor first scheduled run

## Plugin Testing UI

The app includes a plugin tester:
- Test search queries
- Inspect scraped data
- Test chapter extraction
- Preview image list
- Validate metadata extraction
