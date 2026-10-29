# Linux environments

This feature runs trusted ARM64 Linux userlands inside a chroot. It is intended for
local development tools and foreground services. It shares the Android kernel,
network, UID space and device resources. It does not create a security sandbox or
apply CPU or memory quotas. Guest ROOT can affect Android; import only rootfs images
and commands you trust.

## Requirements and import

Changes require an ARM64 Android build, ROOT, mount/chroot privileges and sufficient
SELinux access. Process containment requires either PID namespaces or an existing,
writable cgroup v2 hierarchy at `/sys/fs/cgroup` with `cgroup.kill` (Linux 5.14+).
`pidfd_open` and `pidfd_send_signal` must also be allowed. A kernel version string
alone does not establish that these facilities are enabled. The backend never
changes SELinux enforcement or mounts/reconfigures Android's cgroup hierarchy.

Use a local `.tar` or `.tar.gz` root filesystem, such as an official ARM64 minimal
distribution image. Limits are 4 GiB for the source archive and any individual file,
16 GiB of expanded payload, 250,000 headers and 16 environments. Required programs
are executable ARM64 ELF files at `/bin/sh`, `/bin/sleep` and `/bin/cat`; symlinks
within the guest are resolved for this check. A full disk image is unsupported.

Inspection computes SHA-256. Import copies and verifies a private snapshot before
extracting it, so changes after inspection fail verification. A raw header pass
bounds GNU/PAX extension allocation before extraction. Traversal, writes through
links outside staging, device nodes, sparse and other special entries are rejected.
Relative paths and normal symbolic/hard links are supported; archives that require
writing through an absolute directory symlink must store files under its real guest
target instead. Ownership and Linux permission bits are restored on Android;
extended attributes, ACLs and file capabilities are not restored.

Only a complete, validated environment is published by directory rename. Staging
and metadata are private; interrupted staging is removed during the next import.
Cancellation and the 30-minute deadline are checked between copy/decompression
reads as well as archive entries. A blocked filesystem operation can delay these
cooperative checks. Failure leaves the source archive unchanged and publishes no
partial environment. A crash after publication can leave a completed environment
with an interrupted job result; the environment list is authoritative in that case.

## Runtime and lifecycle

A supervisor starts before any asynchronous runtime threads. It creates a private
mount namespace and disables mount propagation before setting up guest mounts:

- A minimal tmpfs `/dev` with null, zero, random, urandom, tty and separate devpts.
- tmpfs `/run` (16 MiB) and `/tmp` (128 MiB), recreated for each start.
- A non-recursive, read-only `/sys` bind, excluding Android's nested cgroup mounts.
- A guest `/proc` for PID-namespace mode; a read-only host `/proc` bind for the cgroup
  fallback. The fallback exposes Android process information.
- An optional private read-only `/etc/resolv.conf` bind from up to four IP addresses.
  Empty DNS configuration preserves the image's existing resolver configuration.

A pinned, hash-verified static ARM64 Tini v0.19.0 runs as guest PID 1 when PID
namespaces are available, or as a child subreaper otherwise. Up to eight configured
shell commands run in the foreground. They are not systemd units and are not
restarted automatically. Commands should keep their workloads in the foreground.
Combined stdout/stderr logs are bounded separately to 1 MiB per stream per start;
excess output continues to be drained. Configuration changes are saved for the
next start; an already queued start retains the configuration it captured.

The terminal button copies `su -c '…/bin/duckd linux-env shell <id>'` for execution
in a terminal app. This command inherits terminal input/output and joins the running
environment. It intentionally does not use the JSON API output format. Internal
supervisor and import-worker commands also bypass the asynchronous JSON dispatcher.

Stopping signals a verified supervisor through a pidfd, then escalates if needed.
Boot ID and process start time guard against PID reuse. With PID namespaces, init
termination kills the namespace's descendants. Without them, guest children and
terminal sessions join a per-instance cgroup before exec; `cgroup.kill` and
`cgroup.events populated` provide final cleanup and verification. A supervisor
crash can leave fallback descendants until Stop runs; this is reported as unknown
state and blocks starting over or deleting data. These mechanisms do not constrain
a deliberately hostile ROOT process from escaping its containment.

Deletion requires current confirmation, an unused environment lease, no remaining
guest processes and no external host mount under the environment directory.
Uninstall first cancels import and stops environments; unverified cleanup preserves
the data root. Boot startup is opt-in and attempts each environment instance only
once per Android boot, including failed attempts. Runtime capability or policy
failures remain visible and are not worked around with permissive mode.

## Verification

Host checks cover archive identity, confinement, gzip integrity, guest symlinks,
configuration, private state and CLI/UI contracts. WebUI checks cover all 16 locales,
RTL, 320px screens, large text and confirmation layouts. These do not establish
compatibility with an arbitrary vendor kernel or root manager.

The device integration script requires an explicitly selected **disposable rooted
Android VM** and an independently verified local rootfs. It changes no SELinux mode:

```sh
python3 scripts/prepare-linux-env.py
python3 scripts/test-linux-env-device.py \
  --serial emulator-5580 \
  --binary target/aarch64-linux-android/debug/duckd \
  --rootfs /absolute/path/rootfs.tar.gz \
  --sha256 <verified-archive-sha256>
```

Verified with Android API 36 ARM64, kernel
`6.6.66-android15-8-gd0c43a640eab-ab13812146-4k`, SELinux Enforcing, and official
Alpine minirootfs 3.23.7 aarch64 (SHA-256
`7dcc7420bcc426a615c0512cc85b4a52e9f89df9c600b93ce78d22e0e9234891`). This kernel has
`CONFIG_PID_NS` disabled, exercising the cgroup fallback. The script checks import,
permissions, stale revision rejection, services, DNS, terminal entry, termination,
supervisor loss, boot replay protection and deletion. PID namespace mode and vendor
KernelSU/APatch/Magisk policies still require device-specific verification.

## Technical references

- [ACK ARM64 syscall header](https://android.googlesource.com/kernel/common/+/refs/heads/android14-6.1/arch/arm64/include/uapi/asm/unistd.h)
  includes [asm-generic numbers](https://android.googlesource.com/kernel/common/+/refs/heads/android14-6.1/include/uapi/asm-generic/unistd.h):
  `mount=40`, `chroot=51`, `unshare=97`, `setns=268`,
  `pidfd_send_signal=424`, `pidfd_open=434`. Compile-time assertions check the used
  AArch64 ABI; ARM32 uses a different table and is not enabled for mutation.
- [AOSP bionic syscall declarations](https://android.googlesource.com/platform/bionic/+/refs/heads/main/libc/SYSCALLS.TXT),
  [ACK PID namespace teardown](https://android.googlesource.com/kernel/common/+/refs/heads/android14-6.1/kernel/pid_namespace.c),
  [namespace setup](https://android.googlesource.com/kernel/common/+/refs/heads/android14-6.1/kernel/nsproxy.c),
  [cgroup v2 interfaces](https://docs.kernel.org/admin-guide/cgroup-v2.html).
- Linux manual pages: [chroot](https://man7.org/linux/man-pages/man2/chroot.2.html),
  [setns](https://man7.org/linux/man-pages/man2/setns.2.html),
  [pidfd_open](https://man7.org/linux/man-pages/man2/pidfd_open.2.html),
  [PID namespaces](https://man7.org/linux/man-pages/man7/pid_namespaces.7.html).
- [tar unpack_in contract](https://docs.rs/tar/0.4.46/tar/struct.Entry.html#method.unpack_in),
  [Tini v0.19.0 source/release](https://github.com/krallin/tini/releases/tag/v0.19.0),
  [Alpine ARM64 release files](https://dl-cdn.alpinelinux.org/alpine/v3.23/releases/aarch64/).
- Interface guidance: [Material 3 Expressive](https://m3.material.io/blog/building-with-m3-expressive),
  [Material dialogs](https://m3.material.io/components/dialogs/accessibility),
  [Android accessibility targets](https://developer.android.com/guide/topics/ui/accessibility/views/apps-views),
  [Apple alerts](https://developer.apple.com/design/human-interface-guidelines/alerts),
  [Apple accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility),
  [Apple right-to-left guidance](https://developer.apple.com/design/human-interface-guidelines/right-to-left).
