import { useMemo } from 'react';
import type { Book } from '@/types';

interface WallCoverProps {
  books: Book[];
  coverMap: Record<string, string | null>;
}

const COLS = 3;
const ROWS = 7;

interface Tile {
  id: string;
  title: string;
  src: string | null;
}

export function WallCover({ books, coverMap }: WallCoverProps) {
  const columns = useMemo<Tile[][]>(() => {
    const out: Tile[][] = Array.from({ length: COLS }, () => []);
    const n = books.length;
    if (n === 0) return out;
    for (let r = 0; r < ROWS; r++) {
      for (let c = 0; c < COLS; c++) {
        const book = books[(r * COLS + c) % n];
        out[c].push({
          id: book.id,
          title: book.original_filename || book.title,
          src: coverMap[book.id] ?? null,
        });
      }
    }
    return out;
  }, [books, coverMap]);

  return (
    <div className="wall">
      <div className="wall__cols">
        {columns.map((col, ci) => (
          <div
            key={ci}
            className={`wall__col ${ci % 2 === 0 ? 'wall__col--down' : 'wall__col--up'}`}
          >
            <div className="wall__track">
              {/* Duplicate twice for seamless looping */}
              {[0, 1].map((dup) => (
                <div key={dup}>
                  {col.map((tile, ri) => (
                    <div key={`${ci}-${dup}-${ri}-${tile.id}`} className="wall__tile">
                      {tile.src && (
                        <img
                          src={tile.src}
                          alt={tile.title}
                          className="wall__img"
                          loading="lazy"
                        />
                      )}
                    </div>
                  ))}
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}