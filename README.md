# Chy Google Drive Desktop Backup Client

A high-performance desktop backup application built with **Tauri v2 (Rust)** and **Nuxt 3 / Vue 3 (Nuxt UI)** by Chy Bilgisayar. It monitors selected local folders in the background, detects file changes, and automatically backs them up to your Google Drive account.

> 🇹🇷 [Türkçe README için aşağıya bakın](#-türkçe)

---

## 🚀 Features

### 🔒 Secure Google OAuth Authentication
Sign in with your Google account in one click. Login state, profile picture, name, and email are displayed dynamically throughout the UI.

### 🖥️ Device-based Folder Grouping
The app auto-detects your computer's hostname and organises backups in Google Drive as `Chy Drive Backup / [Device-Name]`. Multiple computers can safely share the same account without mixing backups.

### 📂 Full Folder Hierarchy Replication
Sub-folder structures are mirrored to Google Drive exactly as they exist locally. An in-memory folder cache prevents redundant Drive API calls, keeping sync fast and efficient.

### 👁️ Real-time File Watcher with Re-sync
The `notify` crate watches selected folders recursively. When a file is **added or modified**, the watcher:
1. Debounces events for 800 ms (waits for filesystem activity to settle)
2. Marks changed files as **dirty** (removes them from the sync DB so they are re-uploaded on the next run)
3. Triggers a background re-scan and sync automatically

### ⏸ Pause / Resume Sync
An active sync can be paused at any time from the dashboard. All in-flight upload tasks wait in a 200 ms poll loop until resumed. The UI button state persists across page navigations.

### 🐢 Throttled Upload Queue
Optionally spread large uploads over time to avoid performance impact:
- **Chunk Size** — how many files to upload per batch (default: 10)
- **Interval** — seconds to wait between batches (default: 5 s)
- Cancel and pause flags are respected between every chunk

### 📅 Backup Scheduler
Configure automatic backups on a recurring schedule:
- **Day selector** — choose any combination of weekdays (Sun–Sat)
- **Time picker** — set the local time (24-hour format)
- The backend scheduler checks every 30 seconds and fires a sync when the schedule matches. Double-firing within the same minute is prevented.
- The dashboard shows **Next scheduled sync** time.

### 📊 Live Statistics Cards
| Card | Description |
|---|---|
| **Sync Status** | Idle / Checking / Syncing / Paused states with icons |
| **Storage Used** | Google Drive quota with usage bar |
| **Local Files** | Total file count and size from the local scan |
| **Sync Progress** | Per-session progress bar (files uploaded / total) |
| **Network Activity** | Upload / download rate display |

### ⚙️ Ignore Rules
- **Extension-based**: skip `.tmp`, `.log`, `.DS_Store`, etc.
- **Regex-based**: patterns like `^node_modules/.*` prune entire directory trees at traversal time — the directory itself is detected by testing both `path` and `path/` against each pattern.

### 🗄️ SQLite Sync Log
Every sync action is recorded locally (`sync_logs.sqlite`). The **Recent Activity** panel shows the last 50 events with status icons. One-click history clear is available.

---

## 🛠️ Tech Stack

| Layer | Technology |
|---|---|
| Frontend | Nuxt 3, Vue 3, Nuxt UI v4 |
| Backend / Core | Rust, Tauri v2 |
| Event Bridge | Tauri Emitter (Rust → Vue event streaming) |
| Database | SQLite via `rusqlite` |
| File Watching | `notify` crate (cross-platform recursive watcher) |
| Filtering | `regex` crate |
| Scheduling | `chrono` crate + custom 30 s poll loop |
| Parallelism | `rayon` (parallel directory scan), `tokio` semaphore (upload concurrency) |

---

## 📦 Installation & Setup

### Prerequisites
- Node.js v18+
- Rust & Cargo (latest stable)
- macOS Xcode Command Line Tools **or** Windows Build Tools

### Install Dependencies
```bash
npm install
```

### Development Mode
```bash
# Starts Tauri dev server + Nuxt hot-reload frontend
npm run tauri dev
```

### Production Build
```bash
# Creates a platform-specific installer (.dmg / .app / .msi)
npm run tauri build
```

---

## ⚙️ Configuration & Data Paths

| Item | Location |
|---|---|
| Config file | `<AppConfigDir>/config.json` (sync folders, ignore rules, throttle, schedule) |
| Sync log DB | `<AppDataDir>/sync_logs.sqlite` |
| OAuth token | OS Keychain (macOS Keychain / Windows Credential Manager) |

---

---

## 🇹🇷 Türkçe

**Chy Google Drive Desktop Backup Client**, Tauri v2 (Rust) ve Nuxt 3 / Vue 3 ile geliştirilmiş yüksek performanslı bir masaüstü yedekleme uygulamasıdır.

---

## 🚀 Özellikler

### 🔒 Güvenli Google OAuth Girişi
Google hesabınızla tek tıkla giriş. Giriş durumu, profil resmi ve e-posta tüm ekranlarda dinamik olarak gösterilir.

### 🖥️ Cihaz Bazlı Klasör Gruplama
Bilgisayar adı (hostname) otomatik algılanır, yedekler Drive'da `Chy Drive Backup / [Bilgisayar-Adi]` altında organize edilir.

### 📂 Klasör Yapısını Birebir Kopyalama
Alt klasör hiyerarşisi Google Drive'a birebir aktarılır. Bellek önbelleği gereksiz API çağrılarını engeller.

### 👁️ Gerçek Zamanlı Dosya İzleyici & Re-sync
`notify` kütüphanesi klasörleri izler. Dosya eklendiğinde veya değiştirildiğinde:
1. 800 ms debounce beklenir
2. Değişen dosyalar DB'den silinerek **dirty** işaretlenir (bir sonraki sync'te yeniden yüklenir)
3. Arka planda tarama ve senkronizasyon otomatik başlar

### ⏸ Senkronizasyonu Durdur / Devam Et
Aktif sync istediğiniz zaman duraklatılabilir. Tüm yükleme görevleri resume edilene kadar 200 ms poll döngüsünde bekler. Buton durumu sayfa değişimlerinde korunur.

### 🐢 Kısıtlamalı Yükleme Kuyruğu (Throttle)
Büyük yüklemeleri zamana yayarak performans etkisini azaltın:
- **Chunk Boyutu** — her seferde kaç dosya yükleneceği (varsayılan: 10)
- **Aralık** — batch'ler arasında bekleme süresi (varsayılan: 5 saniye)

### 📅 Yedekleme Zamanlaması
Otomatik yedekleme planı oluşturun:
- **Gün seçici** — haftanın istediğiniz günlerini seçin
- **Saat/dakika** — 24 saatlik formatta yerel saat belirtin
- Arka plan scheduler her 30 saniyede kontrol eder ve eşleşince sync başlatır
- Dashboard'da **Sonraki yedekleme** zamanı gösterilir

### ⚙️ Yoksayma Kuralları
- **Uzantı bazlı**: `.tmp`, `.log`, `.DS_Store` vb. atlanır
- **Regex bazlı**: `^node_modules/.*` gibi desenler dizin ağaçlarını baştan keser — dizinin kendisi `path` ve `path/` olarak her iki şekilde de test edilir

### 🗄️ SQLite Senkronizasyon Günlüğü
Her işlem yerel olarak kaydedilir. Son 50 kayıt "Recent Activity" panelinde görüntülenir.

---

## 🛠️ Teknoloji Yığını

| Katman | Teknoloji |
|---|---|
| Frontend | Nuxt 3, Vue 3, Nuxt UI v4 |
| Backend / Core | Rust, Tauri v2 |
| Olay Köprüsü | Tauri Emitter (Rust → Vue) |
| Veritabanı | SQLite (`rusqlite`) |
| Dosya İzleme | `notify` crate |
| Filtreleme | `regex` crate |
| Zamanlama | `chrono` crate + 30 sn poll döngüsü |
| Paralellik | `rayon` (tarama), `tokio` semaphore (yükleme) |

---

## 📦 Kurulum

### Gereksinimler
- Node.js v18+
- Rust & Cargo (son kararlı sürüm)
- macOS Xcode Command Line Tools veya Windows Build Tools

### Bağımlılıkları Yükle
```bash
npm install
```

### Geliştirici Modu
```bash
npm run tauri dev
```

### Üretim Derlemesi
```bash
npm run tauri build
```

---

## ⚙️ Yapılandırma & Veri Yolları

| Öğe | Konum |
|---|---|
| Config dosyası | `<AppConfigDir>/config.json` |
| Senkronizasyon DB | `<AppDataDir>/sync_logs.sqlite` |
| OAuth token | İşletim sistemi Keychain'i |
