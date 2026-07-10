export interface Book {
  id: string;
  title: string;
  original_filename?: string;
  file_path: string;
  file_size: number;
  format: 'cb7' | 'cbz' | 'cbr' | 'pdf';
  page_count: number;
  cover_path?: string;
  source_plugin?: string;
  source_url?: string;
  source_post_id?: string;
  author?: string;
  author_id?: string;
  published_at?: string;
  scraped_at?: string;
  created_at: string;
  updated_at: string;
  last_read_at?: string;
  read_count: number;
  tags?: string;
}

export interface Tag {
  id: string;
  name: string;
  tag_type: 'genre' | 'artist' | 'author' | 'series' | 'custom';
  created_at: string;
}

/** A tag with the number of books linked to it, for the tag-chip filter row. */
export interface TagCount {
  name: string;
  count: number;
}

export interface Collection {
  id: string;
  name: string;
  description?: string;
  created_at: string;
}

export interface BookMetadata {
  title: string;
  author?: string;
  artist?: string;
  description?: string;
  tags: string[];
  status?: string;
  rating?: number;
}

export interface SearchQuery {
  text?: string;
  tags?: string[];
  tags_any?: string[];
  collections?: string[];
  date_from?: string;
  date_to?: string;
  sources?: string[];
  sort_by: 'relevance' | 'title' | 'date' | 'size';
  sort_order: 'asc' | 'desc';
  page: number;
  page_size: number;
}

export interface SearchFacets {
  tags: Tag[];
  collections: Collection[];
  sources: string[];
}

export interface SearchResult {
  books: Book[];
  total: number;
  facets: SearchFacets;
}

/** A Pixiv artwork shown in the 关注/收藏 browse grid (mirrors backend UserWork). */
export interface PixivWork {
  id: string;
  title: string;
  tags: string[];
  pageCount: number;
  illustType?: number;
  author?: string;
  authorId?: string;
  publishedAt?: string;
  coverUrl?: string;
}

/** Local state of a Pixiv work in the browse grid: already downloaded, currently
 *  downloading, or neither (mirrors backend PixivBrowseStatus). */
export interface PixivBrowseStatus {
  workId: string;
  localBookId?: string;
  taskId?: string;
  taskStatus?: string;
  progressCurrent: number;
  progressTotal: number;
}
