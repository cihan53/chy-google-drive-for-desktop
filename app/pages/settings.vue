<script setup>
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { onMounted, ref, computed } from 'vue';

const isAuthenticated = useState('auth');
const userProfile = useState('user-profile');

const config = ref({
  excluded_extensions: [],
  regex_paths: [],
  sync_folders: [],
  bandwidth_limit_enabled: false,
  max_upload_rate_kb: 0,
  max_download_rate_kb: 0,
  throttle_enabled: false,
  throttle_chunk_size: 10,
  throttle_interval_secs: 5,
  schedule: null,
});

const excludedExtsStr = ref('');
const regexPathsStr = ref('');

// Schedule local state (separate from config.value.schedule for ease of binding)
const scheduleEnabled = ref(false);
const scheduleDays = ref([1, 2, 3, 4, 5]); // Mon-Fri default
const scheduleHour = ref(23);
const scheduleMinute = ref(0);

const DAY_LABELS = [
  { value: 0, label: 'Sun' },
  { value: 1, label: 'Mon' },
  { value: 2, label: 'Tue' },
  { value: 3, label: 'Wed' },
  { value: 4, label: 'Thu' },
  { value: 5, label: 'Fri' },
  { value: 6, label: 'Sat' },
];

const toggleDay = (day) => {
  const idx = scheduleDays.value.indexOf(day);
  if (idx >= 0) {
    scheduleDays.value.splice(idx, 1);
  } else {
    scheduleDays.value.push(day);
    scheduleDays.value.sort();
  }
};

const nextScheduledLabel = computed(() => {
  if (!scheduleEnabled.value || scheduleDays.value.length === 0) return null;

  const now = new Date();
  for (let offset = 0; offset <= 7; offset++) {
    const candidate = new Date(now);
    candidate.setDate(now.getDate() + offset);
    candidate.setHours(scheduleHour.value, scheduleMinute.value, 0, 0);

    const jsDay = candidate.getDay(); // 0=Sun…6=Sat
    if (!scheduleDays.value.includes(jsDay)) continue;
    if (candidate <= now) continue;

    return candidate.toLocaleString('tr-TR', {
      weekday: 'long',
      hour: '2-digit',
      minute: '2-digit',
    });
  }
  return null;
});

const loadConfig = async () => {
  try {
    const data = await invoke('load_config');
    config.value = {
      ...config.value,
      ...data,
      throttle_chunk_size: data.throttle_chunk_size ?? 10,
      throttle_interval_secs: data.throttle_interval_secs ?? 5,
    };
    excludedExtsStr.value = data.excluded_extensions.join(', ');
    regexPathsStr.value = data.regex_paths.join('\n');

    // Hydrate schedule local state
    if (data.schedule) {
      scheduleEnabled.value = data.schedule.enabled ?? false;
      scheduleDays.value = data.schedule.days ?? [1, 2, 3, 4, 5];
      scheduleHour.value = data.schedule.hour ?? 23;
      scheduleMinute.value = data.schedule.minute ?? 0;
    }
  } catch (error) {
    console.error('Failed to load config:', error);
  }
};

const saveConfig = async () => {
  try {
    config.value.excluded_extensions = excludedExtsStr.value.split(',').map(s => s.trim()).filter(Boolean);
    config.value.regex_paths = regexPathsStr.value.split('\n').map(s => s.trim()).filter(Boolean);

    // Merge schedule back into config
    config.value.schedule = {
      enabled: scheduleEnabled.value,
      days: scheduleDays.value,
      hour: Number(scheduleHour.value),
      minute: Number(scheduleMinute.value),
    };

    await invoke('save_config', { config: config.value });
    alert('Settings saved successfully!');
  } catch (error) {
    console.error('Failed to save config:', error);
  }
};

const selectFolders = async () => {
  try {
    const selected = await open({
      directory: true,
      multiple: true,
    });
    if (selected) {
      const paths = Array.isArray(selected) ? selected : [selected];
      const uniquePaths = new Set([...(config.value.sync_folders || []), ...paths]);
      config.value.sync_folders = Array.from(uniquePaths);
    }
  } catch (err) {
    console.error('Failed to open dialog:', err);
  }
};

const removeFolder = (index) => {
  if (config.value.sync_folders) {
    config.value.sync_folders.splice(index, 1);
  }
};

const checkAuth = async () => {
  try {
    const authenticated = await invoke('check_auth_status');
    isAuthenticated.value = authenticated;
    if (authenticated) {
      try {
        userProfile.value = await invoke('get_user_profile');
      } catch (err) {
        console.error('Failed to fetch user profile:', err);
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

onMounted(() => {
  loadConfig();
  checkAuth();
});
</script>

<template>
  <div class="space-y-6">
    <div>
      <h2 class="text-2xl font-bold tracking-tight">Settings</h2>
      <p class="text-gray-500 dark:text-gray-400">Manage your sync preferences and account settings.</p>
    </div>

    <UCard>
      <template #header>
        <h3 class="font-semibold text-lg">Google Account</h3>
      </template>
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-3">
          <UAvatar :src="isAuthenticated ? (userProfile?.picture || 'https://avatars.githubusercontent.com/u/1?v=4') : 'https://avatars.githubusercontent.com/u/1?v=4'" :alt="userProfile?.name || 'Account'" />
          <div>
            <div class="font-medium">{{ isAuthenticated ? (userProfile?.name || 'Connected') : 'Not Connected' }}</div>
            <div class="text-xs text-gray-500">{{ isAuthenticated ? (userProfile?.email || 'Your Google Drive is linked') : 'Sign in to start syncing' }}</div>
          </div>
        </div>
        <UButton v-if="!isAuthenticated" @click="login" icon="i-lucide-log-in" color="primary">Sign In with Google</UButton>
        <UButton v-else @click="logout" icon="i-lucide-log-out" color="red" variant="soft">Sign Out</UButton>
      </div>
    </UCard>

    <UCard>
      <template #header>
        <div class="flex items-center justify-between">
          <h3 class="font-semibold text-lg">Sync Folders</h3>
          <UButton @click="selectFolders" color="primary" variant="soft" icon="i-lucide-folder-plus">Add Folders</UButton>
        </div>
      </template>
      <div class="space-y-3">
        <div v-if="!config.sync_folders || config.sync_folders.length === 0" class="text-sm text-gray-500 py-6 text-center border-2 border-dashed border-gray-200 dark:border-gray-800 rounded-lg">
          No sync folders selected. Click "Add Folders" to select.
        </div>
        <div v-else class="space-y-2 max-h-60 overflow-y-auto">
          <div 
            v-for="(folder, index) in config.sync_folders" 
            :key="folder" 
            class="flex items-center justify-between p-2.5 bg-gray-50 dark:bg-gray-800/50 rounded-lg border border-gray-200 dark:border-gray-800"
          >
            <div class="flex items-center gap-2 overflow-hidden mr-4">
              <UIcon name="i-lucide-folder" class="w-5 h-5 text-primary shrink-0" />
              <span class="text-sm font-mono truncate" :title="folder">{{ folder }}</span>
            </div>
            <UButton 
              @click="removeFolder(index)" 
              icon="i-lucide-trash-2" 
              color="red" 
              variant="ghost" 
              size="xs" 
            />
          </div>
        </div>
      </div>
    </UCard>

    <UCard>
      <template #header>
        <h3 class="font-semibold text-lg">Filters &amp; Exclusions</h3>
      </template>
      <div class="space-y-6">
        <UFormField label="Excluded Extensions" description="Comma separated extensions to ignore (e.g. .tmp, .log)">
          <UInput v-model="excludedExtsStr" placeholder=".tmp, .log, .DS_Store" class="w-full" />
        </UFormField>
        
        <UFormField label="Regex Paths" description="Regex patterns for files/folders to ignore">
          <UTextarea v-model="regexPathsStr" placeholder="^node_modules/.*" class="w-full" />
        </UFormField>
      </div>
    </UCard>

    <UCard>
      <template #header>
        <div class="flex items-center justify-between">
          <h3 class="font-semibold text-lg">Bandwidth Limits</h3>
          <UToggle v-model="config.bandwidth_limit_enabled" />
        </div>
      </template>
      <div class="grid grid-cols-1 md:grid-cols-2 gap-6" :class="{ 'opacity-50 pointer-events-none': !config.bandwidth_limit_enabled }">
        <UFormField label="Max Upload Rate (KB/s)" description="0 for unlimited">
          <UInput type="number" v-model="config.max_upload_rate_kb" placeholder="300" class="w-full" />
        </UFormField>
        <UFormField label="Max Download Rate (KB/s)" description="0 for unlimited">
          <UInput type="number" v-model="config.max_download_rate_kb" placeholder="1024" class="w-full" />
        </UFormField>
      </div>
    </UCard>

    <!-- Throttle Card -->
    <UCard>
      <template #header>
        <div class="flex items-center justify-between">
          <div>
            <h3 class="font-semibold text-lg">Sync Throttle</h3>
            <p class="text-xs text-gray-500 mt-0.5">Spread large uploads over time to avoid performance impact.</p>
          </div>
          <UToggle v-model="config.throttle_enabled" />
        </div>
      </template>
      <div class="grid grid-cols-1 md:grid-cols-2 gap-6" :class="{ 'opacity-50 pointer-events-none': !config.throttle_enabled }">
        <UFormField label="Files per Chunk" description="How many files to upload before pausing">
          <UInput type="number" v-model="config.throttle_chunk_size" :min="1" :max="500" placeholder="10" class="w-full" />
        </UFormField>
        <UFormField label="Pause Between Chunks (seconds)" description="Wait time between upload batches">
          <UInput type="number" v-model="config.throttle_interval_secs" :min="1" :max="3600" placeholder="5" class="w-full" />
        </UFormField>
      </div>
      <div v-if="config.throttle_enabled" class="mt-4 p-3 bg-blue-50 dark:bg-blue-950/30 rounded-lg text-sm text-blue-700 dark:text-blue-300 flex items-center gap-2">
        <UIcon name="i-lucide-info" class="w-4 h-4 shrink-0" />
        <span>
          Uploads <strong>{{ config.throttle_chunk_size }} files</strong> at a time, then waits <strong>{{ config.throttle_interval_secs }}s</strong> before the next batch.
        </span>
      </div>
    </UCard>

    <!-- Backup Schedule Card -->
    <UCard>
      <template #header>
        <div class="flex items-center justify-between">
          <div>
            <h3 class="font-semibold text-lg">Backup Schedule</h3>
            <p class="text-xs text-gray-500 mt-0.5">Automatically back up at a set time each week.</p>
          </div>
          <UToggle v-model="scheduleEnabled" />
        </div>
      </template>

      <div class="space-y-6" :class="{ 'opacity-50 pointer-events-none': !scheduleEnabled }">
        <!-- Day selector -->
        <UFormField label="Days" description="Select which days to run the backup">
          <div class="flex flex-wrap gap-2 mt-1">
            <button
              v-for="day in DAY_LABELS"
              :key="day.value"
              type="button"
              @click="toggleDay(day.value)"
              :class="[
                'px-3 py-1.5 rounded-lg text-sm font-medium border transition-all',
                scheduleDays.includes(day.value)
                  ? 'bg-primary text-white border-primary'
                  : 'bg-gray-50 dark:bg-gray-800 text-gray-600 dark:text-gray-400 border-gray-200 dark:border-gray-700 hover:border-primary/50'
              ]"
            >
              {{ day.label }}
            </button>
          </div>
        </UFormField>

        <!-- Time selector -->
        <UFormField label="Time" description="Local time to start the backup (24-hour format)">
          <div class="flex items-center gap-3 mt-1">
            <UInput
              type="number"
              v-model="scheduleHour"
              :min="0"
              :max="23"
              class="w-24"
              placeholder="23"
            />
            <span class="text-xl font-bold text-gray-400">:</span>
            <UInput
              type="number"
              v-model="scheduleMinute"
              :min="0"
              :max="59"
              class="w-24"
              placeholder="00"
            />
            <span class="text-sm text-gray-500">
              ({{ String(scheduleHour).padStart(2,'0') }}:{{ String(scheduleMinute).padStart(2,'0') }} local time)
            </span>
          </div>
        </UFormField>

        <!-- Next run preview -->
        <div v-if="scheduleEnabled && nextScheduledLabel" class="flex items-center gap-2 p-3 bg-green-50 dark:bg-green-950/30 rounded-lg text-sm text-green-700 dark:text-green-300">
          <UIcon name="i-lucide-clock" class="w-4 h-4 shrink-0" />
          <span>Next scheduled backup: <strong>{{ nextScheduledLabel }}</strong></span>
        </div>
        <div v-else-if="scheduleEnabled && scheduleDays.length === 0" class="flex items-center gap-2 p-3 bg-amber-50 dark:bg-amber-950/30 rounded-lg text-sm text-amber-700 dark:text-amber-300">
          <UIcon name="i-lucide-alert-triangle" class="w-4 h-4 shrink-0" />
          <span>Please select at least one day.</span>
        </div>
      </div>
    </UCard>
    
    <div class="flex justify-end">
      <UButton @click="saveConfig" color="primary" size="lg">Save Settings</UButton>
    </div>
  </div>
</template>
