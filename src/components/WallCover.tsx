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
      <style>{`
        .wall { --tile: calc(100cqw / 3); position: absolute; right: 0; top: 50%; width: 100%; aspect-ratio: 1/1; transform: translateY(-50%); }
        .wall__cols { position: absolute; right: 0; top: 50%; height: 0; transform-origin: right center; transform: rotate(45deg); display: flex; flex-direction: row; align-items: flex-start; gap: 0; }
        .wall__col { flex: 0 0 auto; width: var(--tile); height: calc(var(--tile) * 7); overflow: hidden; }
        .wall__track { display: flex; flex-direction: column; gap: 0; height: calc(var(--tile) * 14); width: var(--tile); }
        .wall__col--down .wall__track { animation: wall-down 40s linear infinite; }
        .wall__col--up .wall__track { animation: wall-up 40s linear infinite; }
        .wall__tile { width: var(--tile); height: var(--tile); flex: 0 0 auto; }
        .wall__img { display: block; width: 100%; height: 100%; object-fit: cover; opacity: 0.5; }
        @keyframes wall-down { from { transform: translateY(0); } to { transform: translateY(-50%); } }
        @keyframes wall-up { from { transform: translateY(-50%); } to { transform: translateY(0); } }
      `}</style>
    </div>
  );
}