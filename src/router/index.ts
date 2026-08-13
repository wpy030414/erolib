import { createRouter, createWebHashHistory } from 'vue-router';

// Lazy-load every view so each route gets its own chunk — the initial bundle
// only ships the router + Home, and other views load on first navigation.
const routes = [
  { path: '/', redirect: '/home' },
  { path: '/home', component: () => import('@/views/Home.vue') },
  { path: '/library', component: () => import('@/views/Library.vue') },
  { path: '/reader/:id', component: () => import('@/views/Reader.vue'), props: true },
  { path: '/pixiv', component: () => import('@/views/PixivDownload.vue') },
  { path: '/ehentai', component: () => import('@/views/EHentai.vue') },
  { path: '/ahentai', component: () => import('@/views/AHentai.vue') },
  { path: '/nicecat', component: () => import('@/views/NiceCat.vue') },
  { path: '/tasks', component: () => import('@/views/Tasks.vue') },
  { path: '/settings', component: () => import('@/views/Settings.vue') },
];

export default createRouter({
  history: createWebHashHistory(),
  routes,
});
