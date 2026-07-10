<template>
  <div id="erolib-app" class="erolib-app d-flex fill-height">
    <AppShell v-if="!isReader" />
    <main class="app-main flex-grow-1">
      <router-view />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { useRoute } from 'vue-router';
import AppShell from './components/AppShell.vue';

const route = useRoute();
const isReader = computed(() => route.path.startsWith('/reader'));
</script>

<style>
.erolib-app {
  min-height: 100vh;
  background: var(--md-sys-color-surface);
}

.app-main {
  display: flex;
  flex-direction: column;
  overflow: auto;
}
</style>
