import { createRouter, createWebHashHistory } from 'vue-router';
import Library from '@/views/Library.vue';
import Reader from '@/views/Reader.vue';
import PixivDownload from '@/views/PixivDownload.vue';
import EHentai from '@/views/EHentai.vue';
import Settings from '@/views/Settings.vue';

const routes = [
  { path: '/', redirect: '/library' },
  { path: '/library', component: Library },
  { path: '/reader/:id', component: Reader, props: true },
  { path: '/pixiv', component: PixivDownload },
  { path: '/ehentai', component: EHentai },
  { path: '/settings', component: Settings },
];

export default createRouter({
  history: createWebHashHistory(),
  routes,
});
