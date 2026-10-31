#!/usr/bin/env bash
# Generates deny.toml (main workspace) and crates/tack-desktop/deny.toml
# (the desktop workspace, its own Cargo.lock) — the single source of truth
# for `cargo deny`'s policy in each, used identically by `make deny` and the
# CI `deny` job so the two can never drift. Neither file is committed (see
# CLAUDE.md: generated artifacts stay out of version control); both are
# regenerated every run instead.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Shared by both workspaces: license allowlist and ban rules. The two
# workspaces never disagree on which licenses or duplicate-version policy
# they accept, only on which advisories they've already reviewed.
common_policy() {
  cat <<'EOF'
[graph]
all-features = true

[bans]
# Duplicate (multiple-version) dependencies are reported as an
# advisory (warning), not a hard failure.
multiple-versions = "warn"
wildcards = "allow"
EOF
  if [ "${1:-}" = "main" ]; then
    main_workspace_bans
  fi
  cat <<'EOF'

[licenses]
version = 2
confidence-threshold = 0.8
allow = [
  "MIT",
  "Apache-2.0",
  "Apache-2.0 WITH LLVM-exception",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "ISC",
  "Zlib",
  "MPL-2.0",
  "Unicode-3.0",
  "Unicode-DFS-2016",
  "CC0-1.0",
  "Unlicense",
  "0BSD",
  "BSL-1.0",
  "OpenSSL",
  "CDLA-Permissive-2.0",
]
EOF
}

# Only the main workspace: the desktop workspace has neither crate. A crate listed in `wrappers`
# is the only kind allowed to depend on the banned one directly, so the SQL driver stays inside
# the store (tack-db) and its test support, and the HTTP framework inside the HTTP layer
# (tack-api; tack-runner takes it only as a dev-dependency, for the fake docket its tests talk to).
main_workspace_bans() {
  cat <<'EOF'
deny = [
  { crate = "sqlx", wrappers = ["tack-db", "tack-test-support"] },
  { crate = "axum", wrappers = ["tack-api", "tack-runner"] },
]
EOF
}

# zbus 6.0.0 folded zvariant and zbus_names into itself, and RustSec marks the
# standalone crates unmaintained. Both workspaces reach them only through the
# Linux credential store (zbus-secret-service-keyring-store 1.0.1 ->
# secret-service 5.2.0 -> zbus 5), and both of those are their newest
# releases, still on zbus 5. Re-review when secret-service moves to zbus 6.
zbus_ignores() {
  cat <<'EOF'
  { id = "RUSTSEC-2026-0370", reason = "zvariant 5.15.0, merged into zbus 6; reached only through secret-service 5.2.0 (newest, on zbus 5) for the Linux credential store. Reviewed 2026-10-17, next review 2027-01-17." },
  { id = "RUSTSEC-2026-0371", reason = "zbus_names 4.3.4, merged into zbus 6; reached only through secret-service 5.2.0 (newest, on zbus 5) for the Linux credential store. Reviewed 2026-10-17, next review 2027-01-17." },
  { id = "RUSTSEC-2026-0372", reason = "zvariant_derive 5.15.0, merged into zbus 6; reached only through secret-service 5.2.0 (newest, on zbus 5) for the Linux credential store. Reviewed 2026-10-17, next review 2027-01-17." },
EOF
}

{
  echo '[advisories]'
  echo 'ignore = ['
  zbus_ignores
  echo ']'
  echo
  common_policy main
} > deny.toml

# crates/tack-desktop pulls in Tauri's Linux WebView backend (wry), which
# still depends on the unmaintained gtk-rs GTK3 bindings and, through
# urlpattern 0.3.0, the unmaintained unic-* Unicode crates. Every entry below
# is reviewed individually rather than exempted as a block; each cites the
# crate, why it's unavoidable at the current Tauri 2.x release, and when to
# check again.
cat > crates/tack-desktop/deny.toml <<'EOF'
[advisories]
ignore = [
  # gtk-rs GTK3 bindings: wry pins gtk = "0.18" and webkit2gtk = "=2.0"
  # through its newest published release (0.56.1), so no Tauri 2.x version
  # moves off GTK3. Re-review when wry publishes a GTK4 backend.
  { id = "RUSTSEC-2024-0413", reason = "atk 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0416", reason = "atk-sys 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0412", reason = "gdk 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0418", reason = "gdk-sys 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0411", reason = "gdkwayland-sys 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0417", reason = "gdkx11 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0414", reason = "gdkx11-sys 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0415", reason = "gtk 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0420", reason = "gtk-sys 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2024-0419", reason = "gtk3-macros 0.18.2, gtk-rs GTK3 binding pinned by wry; no GTK4 backend published yet. Reviewed 2026-09-07, next review 2027-03-07." },
  # Same GTK3 binding line as above, pulled in via glib-macros 0.18.5 rather
  # than directly by wry.
  { id = "RUSTSEC-2024-0370", reason = "proc-macro-error 1.0.4, pulled in by glib-macros 0.18.5 (the gtk-rs GTK3 line); not an independent dependency choice. Reviewed 2026-09-07, next review 2027-03-07." },
  # unic-* Unicode crates: pulled in by urlpattern 0.3.0, which tauri-utils
  # 2.9.3 (Tauri's newest tauri-utils release) pins with `version = "0.3"`.
  # urlpattern 0.6.0 already replaced these with icu_properties; re-review
  # once tauri-utils raises its urlpattern requirement.
  { id = "RUSTSEC-2025-0081", reason = "unic-char-property 0.9.0, pulled in by urlpattern 0.3.0 (pinned by tauri-utils 2.9.3); urlpattern 0.6.0 already dropped it for icu_properties. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2025-0075", reason = "unic-char-range 0.9.0, pulled in by urlpattern 0.3.0 (pinned by tauri-utils 2.9.3); urlpattern 0.6.0 already dropped it for icu_properties. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2025-0080", reason = "unic-common 0.9.0, pulled in by urlpattern 0.3.0 (pinned by tauri-utils 2.9.3); urlpattern 0.6.0 already dropped it for icu_properties. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2025-0100", reason = "unic-ucd-ident 0.9.0, pulled in by urlpattern 0.3.0 (pinned by tauri-utils 2.9.3); urlpattern 0.6.0 already dropped it for icu_properties. Reviewed 2026-09-07, next review 2027-03-07." },
  { id = "RUSTSEC-2025-0098", reason = "unic-ucd-version 0.9.0, pulled in by urlpattern 0.3.0 (pinned by tauri-utils 2.9.3); urlpattern 0.6.0 already dropped it for icu_properties. Reviewed 2026-09-07, next review 2027-03-07." },
EOF
zbus_ignores >> crates/tack-desktop/deny.toml
printf ']\n\n' >> crates/tack-desktop/deny.toml
common_policy >> crates/tack-desktop/deny.toml
