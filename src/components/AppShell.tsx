import { useLocation, Link } from 'react-router-dom';
import { useI18n } from '@/hooks/useI18n';
import { useEhentaiBrowseStore } from '@/stores/ehentai-browse';
import { BrandIcon } from './BrandIcon';
import {
  mdiBookOutline,
  mdiClipboardList,
  mdiCogOutline,
  mdiHome,
} from '@mdi/js';

/** Pixiv brand mark */
const PIXIV_PATH =
  'M4.935 0A4.924 4.924 0 0 0 0 4.935v14.13A4.924 4.924 0 0 0 4.935 24h14.13A4.924 4.924 0 0 0 24 19.065V4.935A4.924 4.924 0 0 0 19.065 0zm7.81 4.547c2.181 0 4.058.676 5.399 1.847a6.118 6.118 0 0 1 2.116 4.66c.005 1.854-.88 3.476-2.257 4.563-1.375 1.092-3.225 1.697-5.258 1.697-2.314 0-4.46-.842-4.46-.842v2.718c.397.116 1.048.365.635.779H5.79c-.41-.41.19-.65.644-.779V7.666c-1.053.81-1.593 1.51-1.868 2.031.32 1.02-.284.969-.284.969l-1.09-1.73s3.868-4.39 9.553-4.39zm-.19.971c-1.423-.003-3.184.473-4.27 1.244v8.646c.988.487 2.484.832 4.26.832h.01c1.596 0 2.98-.593 3.93-1.533.952-.948 1.486-2.183 1.492-3.683-.005-1.54-.504-2.864-1.42-3.86-.918-.992-2.274-1.645-4.002-1.646Z';

const EHENTAI_PATH =
  'M50 50 h20 v20 h-20 Z M20 100 h30 v20 h-30 Z M0 0 h20 v120 h-20 Z M20 0 h30 v20 h-30 Z M80 0 h20 v120 h-20 Z M120 0 h20 v120 h-20 Z M100 50 h20 v20 h-20 Z M20 50 h20 v20 h-20 Z';

const AHENTAI_PATH =
  'M6.0,0.0h9.0v1.5h-9.0Z M19.5,0.0h4.5v1.5h-4.5Z M6.0,1.5h9.0v1.5h-9.0Z M19.5,1.5h4.5v1.5h-4.5Z M6.0,3.0h9.0v1.5h-9.0Z M19.5,3.0h4.5v1.5h-4.5Z M4.5,4.5h10.5v1.5h-10.5Z M19.5,4.5h4.5v1.5h-4.5Z M4.5,6.0h10.5v1.5h-10.5Z M19.5,6.0h4.5v1.5h-4.5Z M4.5,7.5h10.5v1.5h-10.5Z M19.5,7.5h4.5v1.5h-4.5Z M4.5,9.0h19.5v1.5h-19.5Z M3.0,10.5h21.0v1.5h-21.0Z M3.0,12.0h4.5v1.5h-4.5Z M9.0,12.0h15.0v1.5h-15.0Z M3.0,13.5h4.5v1.5h-4.5Z M9.0,13.5h6.0v1.5h-6.0Z M19.5,13.5h4.5v1.5h-4.5Z M1.5,15.0h13.5v1.5h-13.5Z M19.5,15.0h4.5v1.5h-4.5Z M1.5,16.5h13.5v1.5h-13.5Z M19.5,16.5h4.5v1.5h-4.5Z M1.5,18.0h13.5v1.5h-13.5Z M19.5,18.0h4.5v1.5h-4.5Z M1.5,19.5h4.5v1.5h-4.5Z M9.0,19.5h6.0v1.5h-6.0Z M19.5,19.5h4.5v1.5h-4.5Z M0.0,21.0h6.0v1.5h-6.0Z M9.0,21.0h6.0v1.5h-6.0Z M19.5,21.0h4.5v1.5h-4.5Z M0.0,22.5h6.0v1.5h-6.0Z M9.0,22.5h6.0v1.5h-6.0Z M19.5,22.5h4.5v1.5h-4.5Z';

const NICECAT_PATH =
  'M17.2,0.3h2.6v1.3h-2.6ZM2.9,1.6h2.6v1.3h-2.6ZM15.9,1.6h3.9v1.3h-3.9ZM4.2,2.9h2.6v1.3h-2.6ZM17.2,2.9h3.9v1.3h-3.9ZM4.2,4.2h3.9v1.3h-3.9ZM15.9,4.2h3.9v1.3h-3.9ZM2.9,5.5h16.9v1.3h-16.9ZM2.9,6.8h16.9v1.3h-16.9ZM2.9,8.1h16.9v1.3h-16.9ZM2.9,9.4h16.9v1.3h-16.9ZM22.4,9.4h1.3v1.3h-1.3ZM4.2,10.7h15.6v1.3h-15.6ZM21.1,10.7h2.6v1.3h-2.6ZM4.2,12h15.6v1.3h-15.6ZM21.1,12h1.3v1.3h-1.3ZM4.2,13.3h18.2v1.3h-18.2ZM4.2,14.6h18.2v1.3h-18.2ZM4.2,15.9h18.2v1.3h-18.2ZM4.2,17.2h15.6v1.3h-15.6ZM21.1,17.2h1.3v1.3h-1.3ZM4.2,18.5h15.6v1.3h-15.6ZM21.1,18.5h2.6v1.3h-2.6ZM2.9,19.8h16.9v1.3h-16.9ZM22.4,19.8h1.3v1.3h-1.3ZM4.2,21.1h19.5v1.3h-19.5ZM5.5,22.4h16.9v1.3h-16.9Z';

interface NavItem {
  path: string;
  labelKey: 'nav.home' | 'nav.library' | 'nav.pixiv' | 'nav.ehentai' | 'nav.ahentai' | 'nav.nicecat' | 'nav.tasks' | 'nav.settings';
  icon: string;
  viewBox?: string;
  fillRule?: 'nonzero' | 'evenodd';
  brand?: boolean;
}

const navItems: NavItem[] = [
  { path: '/home', labelKey: 'nav.home', icon: mdiHome },
  { path: '/library', labelKey: 'nav.library', icon: mdiBookOutline },
  { path: '/pixiv', labelKey: 'nav.pixiv', icon: PIXIV_PATH, brand: true },
  { path: '/ehentai', labelKey: 'nav.ehentai', icon: EHENTAI_PATH, viewBox: '0 0 140 120', brand: true },
  { path: '/ahentai', labelKey: 'nav.ahentai', icon: AHENTAI_PATH, viewBox: '0 0 24 24', brand: true },
  { path: '/nicecat', labelKey: 'nav.nicecat', icon: NICECAT_PATH, viewBox: '0 0 24 24', brand: true },
  { path: '/tasks', labelKey: 'nav.tasks', icon: mdiClipboardList },
  { path: '/settings', labelKey: 'nav.settings', icon: mdiCogOutline },
];

export function AppShell() {
  const location = useLocation();
  const { t } = useI18n();
  const ex = useEhentaiBrowseStore((s) => s.ex);

  function navLabel(item: NavItem): string {
    if (item.path === '/ehentai') {
      return ex ? t('nav.exhentai') : t('nav.ehentai');
    }
    return t(item.labelKey);
  }

  return (
    <nav className="nav-rail">
      {navItems.map((item) => (
        <Link
          key={item.path}
          to={item.path}
          className={`nav-item${location.pathname === item.path ? ' active' : ''}`}
        >
          <div className="nav-item__icon">
            <BrandIcon
              path={item.icon}
              viewBox={item.viewBox}
              fillRule={item.fillRule}
              brand={item.brand}
              size={24}
            />
          </div>
          <span className="nav-label">{navLabel(item)}</span>
        </Link>
      ))}
    </nav>
  );
}