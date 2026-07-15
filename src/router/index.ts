import { createRouter, createWebHashHistory } from 'vue-router';
import Home from '@/views/Home.vue';
import Library from '@/views/Library.vue';
import Reader from '@/views/Reader.vue';
import PixivDownload from '@/views/PixivDownload.vue';
import EHentai from '@/views/EHentai.vue';
import AHentai from '@/views/AHentai.vue';
import NiceCat from '@/views/NiceCat.vue';
import Tasks from '@/views/Tasks.vue';
import Settings from '@/views/Settings.vue';

const routes = [
  { path: '/', redirect: '/home' },
  { path: '/home', component: Home },
  { path: '/library', component: Library },
  { path: '/reader/:id', component: Reader, props: true },
  { path: '/pixiv', component: PixivDownload },
  { path: '/ehentai', component: EHentai },
  { path: '/ahentai', component: AHentai },
  { path: '/nicecat', component: NiceCat },
  { path: '/tasks', component: Tasks },
  { path: '/settings', component: Settings },
];

export default createRouter({
  history: createWebHashHistory(),
  routes,
});
