# Drive Sync App Guidelines

- **Login Visibility:** Ensure that the user's login state is globally tracked. If the user is not authenticated, display a warning info box on all screens except the Settings screen.
- **System Dialogs:** When selecting folders or files from the file system (e.g., Local Backup Directory), always use native OS dialogs via `@tauri-apps/plugin-dialog` instead of simple text inputs or custom dropdowns.
- **Bandwidth Limits:** The application must have an active/passive toggle for bandwidth limits. Default bandwidth configurations should be set to 300 KB/s for upload and 1 MB/s (1024 KB/s) for download.
- **Profile Menu Location:** The Profile/Google Account widget and authentication controls (Sign In/Out) should be located at the bottom of the left sidebar, near the theme toggle button, rather than isolated in the settings menu.
- **Data Dependency on Auth:** Display metrics like "Storage Used" and "Sync Status" dynamically based on authentication state. Prevent infinite loading or spinning animations when the user is not logged in; instead, show a zero state or an explicit "Not logged in" status.
- **Logo Usage:** Always use the `public/logo.png` image for the application logo (Chy Bilgisayar), replacing default icons in the sidebar or main navigation.
