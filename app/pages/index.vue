<script setup>
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';

const isAuthenticated = useState('auth');
const userProfile = useState('user-profile');

const storage = ref({ limit: '0', usage: '0' });
const loading = ref(false);

const localStats = ref({ fileCount: 0, totalBytes: 0, isDone: true });
const syncedCount = ref(0);
const logs = ref([]);
// Sync state persisted across page navigations via Nuxt useState
const isSyncing = useState('sync-is-syncing', () => false);
const isSyncPaused = useState('sync-is-paused', () => false);
const syncError = useState('sync-error', () => null);
// Tracks the current upload session: { completed, total } emitted by the backend
const sessionProgress = useState('sync-session-progress', () => ({ completed: 0, total: 0 }));
// True while the backend is scanning files but hasn't emitted sync-started yet
const isSyncChecking = useState('sync-is-checking', () => false);
// Schedule config for "next run" preview on dashboard
const scheduleConfig = ref(null);

const fetchStorage = async () => {
  if (isAuthenticated.value) {
    loading.value = true;
    try {
      const data = await invoke('get_storage_quota');
      storage.value = data.storageQuota;
    } catch (err) {
      console.error('Failed to fetch storage quota:', err);
    } finally {
      loading.value = false;
    }
  } else {
    storage.value = { limit: '0', usage: '0' };
    loading.value = false;
  }
};

const fetchSyncedCount = async () => {
  if (isAuthenticated.value) {
    try {
      syncedCount.value = await invoke('get_synced_files_count');
    } catch (err) {
      console.error('Failed to fetch synced files count:', err);
    }
  } else {
    syncedCount.value = 0;
  }
};

const fetchLogs = async () => {
  if (isAuthenticated.value) {
    try {
      logs.value = await invoke('get_recent_logs');
    } catch (err) {
      console.error('Failed to fetch recent logs:', err);
    }
  } else {
    logs.value = [];
  }
};

const startScan = async () => {
  if (isAuthenticated.value) {
    try {
      await invoke('start_local_scan');
    } catch (err) {
      console.error('Failed to start local scan:', err);
    }
  } else {
    localStats.value = { fileCount: 0, totalBytes: 0, isDone: true };
  }
};

const forceSync = async () => {
  // Allow triggering a fresh sync even when paused (paused = just throttled, not cancelled)
  if (!isAuthenticated.value || (isSyncing.value && !isSyncPaused.value)) return;
  isSyncing.value = true;
  isSyncPaused.value = false;
  syncError.value = null;
  try {
    await invoke('trigger_sync');
  } catch (err) {
    console.error('Failed to trigger sync:', err);
    isSyncing.value = false;
  }
};

const pauseSync = async () => {
  try {
    await invoke('pause_sync');
    isSyncPaused.value = true;
  } catch (err) {
    console.error('Failed to pause sync:', err);
  }
};

const resumeSync = async () => {
  try {
    await invoke('resume_sync');
    isSyncPaused.value = false;
  } catch (err) {
    console.error('Failed to resume sync:', err);
  }
};

const checkAndRunAutoSync = async () => {
  if (!isAuthenticated.value || isSyncing.value) return;
  try {
    const started = await invoke('check_and_run_auto_sync');
    if (started) {
      isSyncing.value = true;
    }
  } catch (err) {
    console.error('Failed to check and run auto sync:', err);
  }
};

const clearHistory = async () => {
  if (!isAuthenticated.value) return;
  try {
    await invoke('clear_sync_logs');
    await fetchSyncedCount();
    await fetchLogs();
  } catch (err) {
    console.error('Failed to clear sync logs:', err);
  }
};

const formattedUsage = computed(() => {
  const bytes = parseFloat(storage.value.usage);
  if (isNaN(bytes) || bytes === 0) return '0.0 GB';
  return (bytes / (1024 * 1024 * 1024)).toFixed(1) + ' GB';
});

const formattedLimit = computed(() => {
  const bytes = parseFloat(storage.value.limit);
  if (isNaN(bytes) || bytes === 0) return '0 GB';
  return (bytes / (1024 * 1024 * 1024)).toFixed(0) + ' GB';
});

const usagePercent = computed(() => {
  const usageBytes = parseFloat(storage.value.usage);
  const limitBytes = parseFloat(storage.value.limit);
  if (isNaN(usageBytes) || isNaN(limitBytes) || limitBytes === 0) return 0;
  return Math.round((usageBytes / limitBytes) * 100);
});

const formattedLocalSize = computed(() => {
  const bytes = localStats.value.totalBytes;
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
});

const syncPercentage = computed(() => {
  if (!isAuthenticated.value) return 0;
  // During an active sync: use session-specific progress for a smooth, accurate bar
  if (isSyncing.value && sessionProgress.value.total > 0) {
    return Math.round((sessionProgress.value.completed / sessionProgress.value.total) * 100);
  }
  // Idle: show all-time ratio (synced files / total local files)
  const total = localStats.value.fileCount;
  if (total === 0) return 0;
  const count = Math.min(syncedCount.value, total);
  return Math.round((count / total) * 100);
});

// Compute the next scheduled backup time label for the dashboard
const nextScheduleLabel = computed(() => {
  const sched = scheduleConfig.value;
  if (!sched || !sched.enabled || !sched.days || sched.days.length === 0) return null;
  const now = new Date();
  for (let offset = 0; offset <= 7; offset++) {
    const candidate = new Date(now);
    candidate.setDate(now.getDate() + offset);
    candidate.setHours(sched.hour, sched.minute, 0, 0);
    const jsDay = candidate.getDay();
    if (!sched.days.includes(jsDay)) continue;
    if (candidate <= now) continue;
    return candidate.toLocaleString('tr-TR', { weekday: 'long', hour: '2-digit', minute: '2-digit' });
  }
  return null;
});

const loadScheduleConfig = async () => {
  try {
    const data = await invoke('load_config');
    scheduleConfig.value = data.schedule ?? null;
  } catch (err) {
    console.error('Failed to load schedule config:', err);
  }
};

let unlistenScan = null;
let unlistenSyncChecking = null;
let unlistenSyncStarted = null;
let unlistenSyncProgress = null;
let unlistenStatusUpdate = null;
let unlistenSyncCompleted = null;
let unlistenSyncError = null;

onMounted(async () => {
  fetchStorage();
  fetchSyncedCount();
  fetchLogs();
  startScan();
  loadScheduleConfig();
  if (isAuthenticated.value) {
    checkAndRunAutoSync();
  }

  try {
    unlistenSyncChecking = await listen('sync-checking', () => {
      // Backend started scanning — show the checking indicator
      isSyncChecking.value = true;
      isSyncing.value = true;
      syncError.value = null;
    });
  } catch (err) {
    console.error('Failed to listen to sync-checking:', err);
  }

  try {
    unlistenSyncStarted = await listen('sync-started', (event) => {
      isSyncing.value = true;
      isSyncChecking.value = false; // checking done, uploading starts
      syncError.value = null;
      // Reset session progress with the total count provided by the backend
      const total = event.payload?.total ?? 0;
      sessionProgress.value = { completed: 0, total };
    });
  } catch (err) {
    console.error('Failed to listen to sync started:', err);
  }

  try {
    unlistenSyncProgress = await listen('sync-progress', (event) => {
      // Real-time per-file progress from the backend (completed / total for this session)
      sessionProgress.value = {
        completed: event.payload?.completed ?? 0,
        total: event.payload?.total ?? sessionProgress.value.total,
      };
    });
  } catch (err) {
    console.error('Failed to listen to sync progress:', err);
  }

  try {
    unlistenScan = await listen('sync-scan-progress', (event) => {
      localStats.value = event.payload;
    });
  } catch (err) {
    console.error('Failed to listen to scan progress:', err);
  }

  try {
    // sync-status-update is fired per-file during active sync.
    // Calling fetchSyncedCount() here caused the counter to jump around
    // because multiple concurrent uploads (semaphore=4) raced to update it.
    // Progress during sync is already tracked accurately via sync-progress
    // (backed by AtomicU64 on the Rust side), so we only refresh the DB-based
    // syncedCount after the full sync completes.
    unlistenStatusUpdate = await listen('sync-status-update', () => {
      // No-op during sync — kept for future use (e.g. live log streaming)
    });
  } catch (err) {
    console.error('Failed to listen to sync status updates:', err);
  }

  try {
    unlistenSyncCompleted = await listen('sync-completed', () => {
      isSyncing.value = false;
      isSyncChecking.value = false;
      isSyncPaused.value = false;
      sessionProgress.value = { completed: 0, total: 0 };
      fetchSyncedCount();
      fetchLogs();
      fetchStorage();
    });
  } catch (err) {
    console.error('Failed to listen to sync completed:', err);
  }

  try {
    unlistenSyncError = await listen('sync-error', (event) => {
      syncError.value = event.payload;
    });
  } catch (err) {
    console.error('Failed to listen to sync error:', err);
  }
});

onUnmounted(() => {
  if (unlistenScan) unlistenScan();
  if (unlistenSyncChecking) unlistenSyncChecking();
  if (unlistenSyncStarted) unlistenSyncStarted();
  if (unlistenSyncProgress) unlistenSyncProgress();
  if (unlistenStatusUpdate) unlistenStatusUpdate();
  if (unlistenSyncCompleted) unlistenSyncCompleted();
  if (unlistenSyncError) unlistenSyncError();
});

watch(isAuthenticated, (newVal) => {
  if (newVal) {
    fetchStorage();
    fetchSyncedCount();
    fetchLogs();
    startScan();
    checkAndRunAutoSync();
  } else {
    storage.value = { limit: '0', usage: '0' };
    localStats.value = { fileCount: 0, totalBytes: 0, isDone: true };
    syncedCount.value = 0;
    logs.value = [];
    isSyncing.value = false;
    isSyncChecking.value = false;
    syncError.value = null;
    sessionProgress.value = { completed: 0, total: 0 };
  }
});
</script>

<template>
  <div class="space-y-6">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-2xl font-bold tracking-tight">Status</h2>
        <p class="text-gray-500 dark:text-gray-400">
          <span v-if="isAuthenticated && userProfile?.name" class="font-medium text-gray-800 dark:text-gray-200">Welcome, {{ userProfile.name }}! </span>
          <span>Monitor your backup and synchronization progress.</span>
        </p>
      </div>
      <div class="flex items-center gap-2">
        <!-- Pause / Resume button (only visible while syncing) -->
        <UButton
          v-if="isSyncing"
          :icon="isSyncPaused ? 'i-lucide-play' : 'i-lucide-pause'"
          :color="isSyncPaused ? 'success' : 'warning'"
          variant="soft"
          @click="isSyncPaused ? resumeSync() : pauseSync()"
        >
          {{ isSyncPaused ? 'Resume Sync' : 'Pause Sync' }}
        </UButton>
        <!-- Force Sync button: active when idle OR when paused -->
        <UButton 
          :icon="(isSyncing && !isSyncPaused) ? 'i-lucide-loader-2' : 'i-lucide-play'" 
          :loading="isSyncing && !isSyncPaused"
          :disabled="!isAuthenticated || (isSyncing && !isSyncPaused)"
          color="primary"
          @click="forceSync"
        >
          {{ (isSyncing && !isSyncPaused) ? 'Syncing...' : 'Force Sync Now' }}
        </UButton>
      </div>
    </div>

    <!-- Sync Error Alert -->
    <div v-if="syncError" class="p-4 bg-red-50 dark:bg-red-950/30 text-red-600 dark:text-red-400 text-sm rounded-lg flex items-start gap-2 border border-red-100 dark:border-red-900/30">
      <UIcon name="i-lucide-alert-triangle" class="w-5 h-5 flex-shrink-0 mt-0.5" />
      <div>
        <span class="font-semibold text-red-800 dark:text-red-300">Synchronization Failed:</span> {{ syncError }}
      </div>
    </div>

    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
      <!-- Sync Status -->
      <UCard>
        <template #header>
          <div class="flex items-center gap-2 font-medium">
            <UIcon name="i-lucide-check-circle-2" :class="isAuthenticated ? (isSyncing ? 'text-blue-500' : 'text-green-500') : 'text-gray-400'" class="w-5 h-5" />
            Sync Status
          </div>
        </template>
        <div class="text-2xl font-bold mt-2">
          <span v-if="!isAuthenticated">Not Signed In</span>
          <span v-else-if="isSyncPaused" class="flex items-center gap-2">
            <UIcon name="i-lucide-pause-circle" class="w-5 h-5 text-amber-500" />
            Paused
          </span>
          <span v-else-if="isSyncing" class="flex items-center gap-2">
            <UIcon name="i-lucide-loader-2" class="w-5 h-5 animate-spin text-blue-500" />
            Syncing...
          </span>
          <span v-else>Up to date</span>
        </div>
        <p class="text-sm text-gray-500 mt-1">
          <span v-if="!isAuthenticated">Sign in to start syncing</span>
          <span v-else-if="isSyncPaused">Sync is temporarily paused.</span>
          <span v-else-if="isSyncing">Uploading changes to Google Drive...</span>
          <span v-else>All files are synchronized.</span>
        </p>
        <!-- Next scheduled sync hint -->
        <div v-if="isAuthenticated && !isSyncing && nextScheduleLabel" class="mt-2 flex items-center gap-1.5 text-xs text-gray-400 dark:text-gray-500">
          <UIcon name="i-lucide-clock" class="w-3.5 h-3.5" />
          <span>Next: <span class="font-medium text-gray-500 dark:text-gray-400">{{ nextScheduleLabel }}</span></span>
        </div>
      </UCard>

      <!-- Cloud Storage Used -->
      <UCard>
        <template #header>
          <div class="flex items-center gap-2 font-medium">
            <UIcon name="i-lucide-cloud" :class="isAuthenticated ? 'text-blue-500' : 'text-gray-400'" class="w-5 h-5" />
            Storage Used
          </div>
        </template>
        <div class="text-2xl font-bold mt-2">
          {{ isAuthenticated ? formattedUsage : '0.0 GB' }} <span class="text-sm font-normal text-gray-500">/ {{ isAuthenticated ? formattedLimit : '0 GB' }}</span>
        </div>
        <UProgress v-if="loading" class="mt-3" />
      </UCard>

      <!-- Local Backup Files -->
      <UCard>
        <template #header>
          <div class="flex items-center gap-2 font-medium">
            <UIcon name="i-lucide-hard-drive" :class="isAuthenticated ? 'text-indigo-500' : 'text-gray-400'" class="w-5 h-5" />
            Local Files
          </div>
        </template>
        <div class="text-2xl font-bold mt-2">
          {{ isAuthenticated ? localStats.fileCount : 0 }} <span class="text-sm font-normal text-gray-500">Files</span>
        </div>
        <div class="flex items-center justify-between mt-2 text-sm text-gray-500">
          <span>Total Size: {{ isAuthenticated ? formattedLocalSize : '0 Bytes' }}</span>
          <span v-if="!localStats.isDone && isAuthenticated" class="flex items-center gap-1">
            <UIcon name="i-lucide-loader-2" class="w-4 h-4 animate-spin text-indigo-500" />
            Scanning...
          </span>
        </div>
      </UCard>

      <!-- Sync Progress -->
      <UCard>
        <template #header>
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-2 font-medium">
              <UIcon
                name="i-lucide-refresh-cw"
                :class="[
                  isAuthenticated ? 'text-green-500' : 'text-gray-400',
                  (isSyncing || isSyncChecking) ? 'animate-spin' : ''
                ]"
                class="w-5 h-5"
              />
              Sync Progress
            </div>
            <!-- Checking badge -->
            <span
              v-if="isSyncChecking && isAuthenticated"
              class="flex items-center gap-1 text-xs font-medium text-amber-600 bg-amber-50 dark:bg-amber-900/30 dark:text-amber-400 px-2 py-0.5 rounded-full animate-pulse"
            >
              <UIcon name="i-lucide-search" class="w-3 h-3" />
              Kontrol ediliyor...
            </span>
            <span
              v-else-if="isSyncing && isAuthenticated"
              class="flex items-center gap-1 text-xs font-medium text-blue-600 bg-blue-50 dark:bg-blue-900/30 dark:text-blue-400 px-2 py-0.5 rounded-full"
            >
              <UIcon name="i-lucide-upload-cloud" class="w-3 h-3" />
              Yükleniyor
            </span>
          </div>
        </template>
        <div class="text-2xl font-bold mt-2">
          <template v-if="isSyncChecking && isAuthenticated">
            <span class="text-base font-normal text-amber-500 animate-pulse">
              Dosyalar taranıyor, lütfen bekleyin...
            </span>
          </template>
          <template v-else-if="isSyncing && sessionProgress.total > 0">
            {{ sessionProgress.completed }}
            <span class="text-sm font-normal text-gray-500">/ {{ sessionProgress.total }} Uploading</span>
          </template>
          <template v-else>
            {{ isAuthenticated ? Math.min(syncedCount, localStats.fileCount) : 0 }}
            <span class="text-sm font-normal text-gray-500">/ {{ isAuthenticated ? localStats.fileCount : 0 }} Synced</span>
          </template>
        </div>
        <div class="mt-3">
          <div class="flex justify-between text-xs text-gray-500 mb-1">
            <span>Progress</span>
            <span>{{ isAuthenticated ? syncPercentage : 0 }}%</span>
          </div>
          <UProgress :value="isAuthenticated ? syncPercentage : 0" color="green" />
        </div>
      </UCard>

      <!-- Network Activity -->
      <UCard>
        <template #header>
          <div class="flex items-center gap-2 font-medium">
            <UIcon name="i-lucide-activity" class="text-orange-500 w-5 h-5" />
            Network Activity
          </div>
        </template>
        <div class="flex justify-between items-end mt-2">
          <div>
            <div class="text-sm text-gray-500">Upload</div>
            <div class="font-bold">0 KB/s</div>
          </div>
          <div>
            <div class="text-sm text-gray-500 text-right">Download</div>
            <div class="font-bold text-right">0 KB/s</div>
          </div>
        </div>
      </UCard>
    </div>

    <UCard>
      <template #header>
        <div class="flex items-center justify-between">
          <h3 class="font-semibold text-lg">Recent Activity</h3>
          <UButton 
            v-if="isAuthenticated && logs.length > 0"
            size="xs" 
            color="red" 
            variant="ghost" 
            icon="i-lucide-trash-2"
            @click="clearHistory"
          >
            Clear History
          </UButton>
        </div>
      </template>
      <div v-if="logs.length === 0" class="text-center py-6 text-gray-500">
        No recent activity. Active synchronization logs will appear here.
      </div>
      <div v-else class="space-y-4 max-h-96 overflow-y-auto pr-2">
        <div v-for="(log, idx) in logs" :key="idx" class="flex items-start gap-3 border-b border-gray-100 dark:border-gray-800 pb-3 last:border-b-0 last:pb-0">
          <div class="mt-0.5 p-1.5 rounded-full" :class="log.includes('success') ? 'bg-green-100 dark:bg-green-900/30 text-green-600 dark:text-green-400' : log.includes('error') ? 'bg-red-100 dark:bg-red-900/30 text-red-600' : 'bg-blue-100 dark:bg-blue-900/30 text-blue-600'">
            <UIcon :name="log.includes('success') ? 'i-lucide-check' : log.includes('error') ? 'i-lucide-x' : 'i-lucide-refresh-cw'" class="w-4 h-4" />
          </div>
          <div class="flex-1 min-w-0">
            <p class="font-medium text-sm break-all">{{ log }}</p>
          </div>
        </div>
      </div>
    </UCard>
  </div>
</template>
