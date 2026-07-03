<script setup>
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { onMounted, onUnmounted } from 'vue';

const route = useRoute();
const isAuthenticated = useState('auth', () => false);
const userProfile = useState('user-profile', () => null);

const checkAuth = async () => {
  try {
    const authenticated = await invoke('check_auth_status');
    isAuthenticated.value = authenticated;
    if (authenticated) {
      try {
        userProfile.value = await invoke('get_user_profile');
      } catch (err) {
        console.error('Failed to fetch user profile:', err);
        // Fallback placeholder profile
        userProfile.value = {
          name: 'Connected User',
          email: 'Google Drive Account',
          picture: 'https://avatars.githubusercontent.com/u/1?v=4'
        };
      }
    } else {
      userProfile.value = null;
    }
  } catch (e) {
    console.error('Failed to check auth:', e);
    isAuthenticated.value = false;
    userProfile.value = null;
  }
};

const login = async () => {
  try {
    const res = await invoke('login_with_google');
    console.log('Login result:', res);
    await checkAuth();
  } catch (error) {
    console.error('Login failed:', error);
    alert('Login failed: ' + error);
  }
};

const logout = async () => {
  try {
    await invoke('logout');
    isAuthenticated.value = false;
    userProfile.value = null;
  } catch (e) {}
};

let unlistenFocus = null;
let unlistenAuth = null;

onMounted(async () => {
  checkAuth();
  window.addEventListener('focus', checkAuth);
  try {
    const appWindow = getCurrentWindow();
    unlistenFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
      if (focused) {
        checkAuth();
      }
    });
  } catch (err) {
    console.error('Failed to setup Tauri focus listener:', err);
  }

  try {
    unlistenAuth = await listen('auth-status-changed', (event) => {
      isAuthenticated.value = event.payload;
      if (!event.payload) {
        userProfile.value = null;
      }
    });
  } catch (err) {
    console.error('Failed to setup auth status listener:', err);
  }
});

onUnmounted(() => {
  window.removeEventListener('focus', checkAuth);
  if (unlistenFocus) {
    unlistenFocus();
  }
  if (unlistenAuth) {
    unlistenAuth();
  }
});

useHead({
  meta: [
    { name: 'viewport', content: 'width=device-width, initial-scale=1' }
  ],
  link: [
    { rel: 'icon', href: '/favicon.ico' },
    { rel: 'icon', type: 'image/png', sizes: '16x16', href: '/logo_16.png' },
    { rel: 'icon', type: 'image/png', sizes: '32x32', href: '/logo_32.png' },
    { rel: 'icon', type: 'image/png', sizes: '64x64', href: '/logo_64.png' }
  ],
  htmlAttrs: {
    lang: 'en'
  }
})

const title = 'Chy Bilgisayar Drive Sync'
const description = 'Drive Sync Application'

useSeoMeta({
  title,
  description,
  ogTitle: title,
  ogDescription: description,
  twitterCard: 'summary_large_image'
})
</script>

<template>
  <UApp>
    <div class="flex h-screen overflow-hidden bg-white dark:bg-gray-900 text-gray-900 dark:text-white">
      <!-- Sidebar -->
      <div class="w-64 border-r border-gray-200 dark:border-gray-800 flex flex-col">
        <div class="p-6 border-b border-gray-200 dark:border-gray-800 flex flex-col items-center gap-3">
          <img src="/logo_64.png" srcset="/logo_64.png 1x, /logo_128.png 2x, /logo_256.png 3x" alt="Chy Bilgisayar" class="h-16 w-auto object-contain" />
          <h1 class="font-bold text-lg tracking-tight text-center">Drive Sync</h1>
        </div>
        <nav class="p-4 space-y-2 flex-1">
          <UButton to="/" variant="ghost" color="neutral" block class="justify-start text-base" active-class="bg-gray-100 dark:bg-gray-800 font-medium">
            <template #leading><UIcon name="i-lucide-activity" class="w-5 h-5 mr-2" /></template>
            Status
          </UButton>
          <UButton to="/settings" variant="ghost" color="neutral" block class="justify-start text-base" active-class="bg-gray-100 dark:bg-gray-800 font-medium">
            <template #leading><UIcon name="i-lucide-settings" class="w-5 h-5 mr-2" /></template>
            Settings
          </UButton>
        </nav>
        <div class="p-4 border-t border-gray-200 dark:border-gray-800 flex flex-col gap-4">
          <div v-if="isAuthenticated" class="flex items-center gap-3">
            <UAvatar :src="userProfile?.picture || 'https://avatars.githubusercontent.com/u/1?v=4'" :alt="userProfile?.name || 'Account'" />
            <div class="overflow-hidden flex-1">
              <div class="font-medium text-sm truncate">{{ userProfile?.name || 'Connected' }}</div>
              <div class="text-xs text-gray-500 truncate">{{ userProfile?.email || 'Google Drive' }}</div>
            </div>
            <UButton @click="logout" icon="i-lucide-log-out" size="xs" color="red" variant="ghost" />
          </div>
          <div v-else class="flex items-center justify-between">
            <span class="text-sm font-medium text-gray-500">Not connected</span>
            <UButton @click="login" size="xs" color="primary" variant="soft">Sign In</UButton>
          </div>
          <div class="flex items-center justify-between mt-2">
            <span class="text-sm text-gray-500">Theme</span>
            <UColorModeButton />
          </div>
        </div>
      </div>
      <!-- Main Content -->
      <div class="flex-1 overflow-auto bg-gray-50 dark:bg-gray-950 p-8">
        <UAlert 
          v-if="!isAuthenticated && route.path !== '/settings'" 
          title="Login Required" 
          description="Please sign in via Settings or the sidebar to start syncing your files." 
          color="yellow" 
          variant="soft" 
          icon="i-lucide-alert-triangle" 
          class="mb-6" 
        />
        <NuxtPage />
      </div>
    </div>
  </UApp>
</template>
