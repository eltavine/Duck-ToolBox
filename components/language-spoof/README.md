# Process-local language spoofing

The Miscellaneous tool configures one language per `(Android user ID, package name)`.
Rules live in `/data/adb/duck-toolbox/var/misc/language/rules.json`; an independent
activation record controls whether new processes receive them. Saving does not enable
injection. Disabling preserves the rules, works with malformed rule JSON, and changes
the revision even if already disabled so stale enable requests cannot undo a stop.
Already running processes must be fully closed and reopened after any change.

The CLI exposes `duckd misc language status`, `save --stdin-json`,
`enable --stdin-json` and `disable`. Save input is
`{"revision":"<status revision>","config":{"rules":[{"package_name":"com.example.app","user_id":0,"locale":"en-US"}]}}`.
Enable input is `{"revision":"<latest status revision>"}`. The revision includes
both rules and activation state; old requests fail instead of undoing a later stop.

The independent native/Java component joins the existing Zygisk v4 dispatcher using
channel ID 2. It does not require or enable the Device Faker feature. It runs alongside
PI, and the dispatcher keeps the shared library loaded while any engine owns callbacks.
No hooks run in system_server. System, isolated and SDK-sandbox UIDs are excluded.
The app data directory, process package and numeric UID user scope must agree before
the companion selects a rule. Private companion reads are size bounded and use a
two-second exchange deadline. Missing, corrupt or disabled rules skip injection.

## Language surfaces

- Java `Locale` and `LocaleList` default getters and setters, including locale categories.
- Resource configuration reads and updates. Copies preserve unrelated configuration
  fields; the locale and layout direction follow the selected rule.
- Android `LocaleManager` system and own-application locale reads when present, and the
  framework configuration read through the ActivityManager Java proxy.
- Java `System` language properties and Android `SystemProperties` locale properties.
- Common bionic property getters, reads and read callbacks. A pre-existing unique
  native property hook is preserved when another hook cannot be installed.

LSPlant handles ART method replacement. ShadowHook supplies native trampolines and
the existing xDL implementation resolves ART symbols; a small prefix-search adapter
supports LSPlant's symbol contract. Known framework configuration callers are
deoptimized to reduce bypasses from inlining. Arbitrary precompiled callers, cached
values, direct Binder transactions, native ICU calls and custom app language logic
can still bypass these surfaces. This is not an anti-detection guarantee. No device
language, system property, application data or Android per-app language setting is
written. Regional content and layout may change; advertisement removal is not guaranteed.

## Dependencies and validation

The stable LSPlant 6.4 documentation covers only through Android 15 DP1. The pinned
upstream revision used here documents Android 5–17 and requires NDK r30, libcxx modules
and CMake 3.28 or newer; the combined build uses the installed SDK CMake 4.1.2. LSPlant,
DexBuilder and parallel-hashmap revisions are explicit in the build script. Their
LGPL-3.0 and Apache-2.0 licenses accompany distributions; the build script retrieves
the pinned sources, and packaged source links identify the exact build commit.

Run `python3 scripts/build-play-integrity.py` for the ARM32/ARM64 release build,
`python3 scripts/test-language-spoof.py` for native selection/transport and host-JVM
callback checks, and `python3 scripts/test-zygisk-runtime.py` for dispatcher lifecycle
and descriptor ownership. The host tests do not prove ART or OEM compatibility.

On a test device with working Zygisk, select an ordinary test application and compare
`Locale.getDefault`, both locale categories, `LocaleList.getDefault`, app/system
resource locales, LocaleManager (API 33+) and native locale property reads. Recreate
the Activity, rotate the device, change the device language and restart a remote
process to check configuration updates. Repeat in a secondary Android user, then
disable and restart to verify restoration. An unselected app and system_server must
continue to see the original language. Inspect the `DuckLanguage` log for rejected
hooks. Device validation is required before claiming support for a specific ROM.

The Android framework language API is documented at
<https://developer.android.com/guide/topics/resources/app-languages>.
LSPlant's hook, deoptimization and symbol-resolver contracts are documented at
<https://github.com/LSPosed/LSPlant>.
