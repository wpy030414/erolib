# Module: 标签翻译系统 (Tag Translation)

> 代码生成契约：多语言标签翻译 + 模糊匹配的行为规范。
> 一个 Agent 可独立完成的任务单元 + 约束模块外在行为的可读文档。

## 1. 模块职责

将四个来源的标签（中文/日文/英文/罗马字）统一翻译为当前界面语言，并在搜索/筛选时合并同义标签。

## 2. 核心文件

| 文件 | 职责 |
|---|---|
| `src-tauri/schema/tag_translations.sql` | 1014 条种子 + 表定义 |
| `src-tauri/src/services/locale.rs` | 翻译物化 + display_expr |
| `src-tauri/src/services/similarity.rs` | SIMILARITY UDF（Levenshtein） |
| `src-tauri/src/services/search.rs` | 搜索时使用翻译 |
| `src-tauri/src/commands/settings.rs` | `set_locale` 命令 |

## 3. 数据模型

### tag_translations 表

```sql
CREATE TABLE tag_translations (
    id INTEGER PRIMARY KEY,
    zh TEXT,
    en TEXT,
    ja_hira TEXT,  -- ひらがな
    ja_kata TEXT,  -- カタカナ / 漢字ブロック（日本語表示優先）
    romaji TEXT
);
```

1014 条种子，固定 id，`INSERT OR IGNORE` 幂等。

### 派生表

| 表 | 用途 | 填充时机 |
|---|---|---|
| `v_tag_form`（VIEW） | 任一形式 → 概念 id（UNION 五列） | 查询时 |
| `tag_form_map` | 精确匹配物化（`form TEXT PK COLLATE NOCASE, tid`） | 启动 + 新标签 upsert |
| `tag_resolved` | 精确+模糊总解析（`name TEXT PK COLLATE NOCASE, tid`） | 启动 + 新标签 upsert |

### settings 表

| key | 值 |
|---|---|
| `locale` | `'zh'` / `'en'` / `'ja'` |
| `tag_seed_fp` | 种子指纹（变化时重建派生表） |

## 4. 翻译规则

### 显示列（`display_expr`）

| locale | SQL 表达式 |
|---|---|
| `zh` | `COALESCE(tt.zh, tt.en, tt.romaji, name)` |
| `en` | `COALESCE(tt.en, tt.zh, tt.romaji, name)` |
| `ja` | `COALESCE(tt.ja_kata, tt.ja_hira, tt.romaji, name)` |

无映射时回退原始 name。

### JOIN（`tag_join`）

```sql
LEFT JOIN tag_resolved tr ON tr.name = {alias}.name
LEFT JOIN tag_translations tt ON tt.id = tr.tid
```

## 5. 模糊匹配

### SIMILARITY UDF

- 算法：归一化 Levenshtein，`1 - dist / max(len_a, len_b)`
- 不区分大小写
- 注册：`sqlite3_create_function_v2`（`SQLITE_UTF8 | SQLITE_DETERMINISTIC`）
- 每个池连接 `after_connect` 注册

### 匹配规则

- 精确匹配优先（`tag_form_map`）
- 无精确匹配才走模糊（`tag_resolved` 第二步）
- 模糊阈值：`SIMILARITY ≥ 0.8`
- 领先次优 ≥ 0.03（否则不解析，回退原始名）

## 6. 标签合并

`tags_with_count(text?, collection?)` 查询：

```sql
SELECT {disp} AS name, COUNT(DISTINCT bt.book_id) AS count,
       GROUP_CONCAT(DISTINCT t.name) AS raw_names
FROM tags t
JOIN book_tags bt ON bt.tag_id = t.id
JOIN books b ON b.id = bt.book_id
{join}
GROUP BY {disp}
ORDER BY count DESC, name ASC
LIMIT 30
```

多个 raw name 折叠成一个 chip，`raw_names` 列表用于 `tags_any` 回传。

## 7. Tauri 命令

| 命令 | 参数 | 返回 |
|---|---|---|
| `set_locale` | `{ localeStr }` | `void` |

仅 `zh` / `en` / `ja` 有效，其余落 `zh`。

## 8. 前端行为

- `setLocale(l)` → localStorage → `api.setLocale(l)` fire-and-forget → 触发 `onLocaleChange` 回调
- `onLocaleChange` → `libraryStore.refresh()`（重新拉书库 + 标签计数）
- 语言切换后所有标签相关 UI 刷新

## 9. 物化流程（启动时）

```
1. materialize_form_map:
   INSERT OR IGNORE INTO tag_form_map (form, tid)
   SELECT form, id FROM v_tag_form

2. materialize_resolved:
   a. 精确匹配:
      INSERT OR IGNORE INTO tag_resolved (name, tid)
      SELECT t.name, m.tid FROM tags t JOIN tag_form_map m ON m.form = t.name

   b. 模糊匹配（仅未解析的 tags）:
      INSERT OR IGNORE INTO tag_resolved (name, tid)
      SELECT rest.name, f.id
      FROM rest JOIN v_tag_form f
      WHERE SIMILARITY(rest.name, f.form) >= 0.8
      GROUP BY rest.name
      HAVING MAX(SIMILARITY) - 次优 >= 0.03
```

## 10. 约束

- `tags` / `book_tags` 表永不被翻译修改
- 翻译只发生在读时（SQL JOIN）
- 种子指纹变化时重建派生表（DELETE + 重新物化）
- 新标签 upsert 时调用 `resolve_one_tag` 即时解析

## 11. 相关模块

- [module-library.md](./module-library.md) — 书库（搜索 / 筛选时使用翻译）
- [module-opds-rss.md](./module-opds-rss.md) — 共享服务器（feed 中标签本地化）
