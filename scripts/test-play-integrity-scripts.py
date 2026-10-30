#!/usr/bin/env python3
"""Verify restoration quoting and OEM boot ordering with isolated command doubles."""
import os
from pathlib import Path
import subprocess
import tempfile

SCRIPT_DIR = Path(__file__).resolve().parent
ENGINE = SCRIPT_DIR.parent / "components/play-integrity"
SCRIPTS = ENGINE / "scripts"


def run(script, root, expected_exit=0, **environment):
    result = subprocess.run(["sh", "-c", script], env={
        **os.environ, "PIF_ENGINE_DATA": str(root), "PIF_ENGINE_SCRIPTS": str(SCRIPTS),
        "PIF_ENGINE_ROOT": str(root / "module"), **environment,
    })
    assert result.returncode == expected_exit, result.returncode


with tempfile.TemporaryDirectory(prefix="pif-shell-test-") as directory:
    root = Path(directory)
    payload = "a'\"$(touch " + str(root / "must-not-exist") + ")`id`\\tail"
    run('''
getprop() { printf '1'; }
resetprop() {
  if [ "$#" = 0 ]; then printf '[persist.test]: [%s]\\n' "$TEST_VALUE";
  elif [ "$#" = 1 ]; then printf '%s' "$TEST_VALUE";
  fi
}
. "$PIF_ENGINE_SCRIPTS/common_func.sh"
persistprop persist.test replacement
resetprop() { printf '%s' "$4" > "$PIF_ENGINE_DATA/restored"; }
. "$PIF_ENGINE_DATA/uninstall.sh"
''', root, TEST_VALUE=payload)
    assert (root / "restored").read_text() == payload
    assert not (root / "must-not-exist").exists()
    (root / "uninstall.sh").unlink()

    # Direct lifecycle requests cannot perform the late pass before boot completion.
    run('''
getprop() { printf 0; }
resetprop() { printf unexpected > "$PIF_ENGINE_DATA/premature-late"; }
set -- late
. "$PIF_ENGINE_SCRIPTS/stage.sh"
''', root, expected_exit=1, PIF_ENGINE_MODE="full")
    assert not (root / "premature-late").exists()
    (root / "uninstall.sh").write_text('false\nprintf unexpected > "$PIF_ENGINE_DATA/continued"\n')
    recovery = subprocess.run(["sh", str(SCRIPTS / "stage.sh"), "cleanup"], env={
        **os.environ, "PIF_ENGINE_DATA": str(root), "PIF_ENGINE_SCRIPTS": str(SCRIPTS),
        "PIF_ENGINE_ROOT": str(root / "module"), "PIF_ENGINE_MODE": "full",
    }, capture_output=True)
    assert recovery.returncode != 0
    assert (root / "uninstall.sh").exists()
    assert not (root / "continued").exists()
    (root / "uninstall.sh").unlink()

    run('''
getprop() { printf '1'; }
resetprop() {
  if [ "$#" = 3 ]; then printf '%s' "$*" > "$PIF_ENGINE_DATA/created"; fi
}
. "$PIF_ENGINE_SCRIPTS/common_func.sh"
RESETPROP=false
resetprop_if_empty ro.boot.vbmeta.device_state locked
''', root)
    assert (root / "created").read_text() == "-n ro.boot.vbmeta.device_state locked"

    # Validate the complete list before any whiteout or overlay operation.
    for invalid in ["/", "/system/", "/system/app", "/system/app/..", "/system/app/../*", "/system/overlay/*", "/system/overlay/config"]:
        (root / "custom.app_replace_list.txt").write_text(invalid + "\n")
        rejected = subprocess.run(["sh", "-c", r'''
ui_print() { :; }
mkdir() { printf unsafe > "$PIF_ENGINE_DATA/mutated"; }
touch() { printf unsafe > "$PIF_ENGINE_DATA/mutated"; }
mknod() { printf unsafe > "$PIF_ENGINE_DATA/mutated"; }
MODPATH=$PIF_ENGINE_ROOT
. "$PIF_ENGINE_SCRIPTS/common_func.sh"
. "$PIF_ENGINE_SCRIPTS/common_setup.sh"
'''], env={**os.environ,"PIF_ENGINE_DATA":str(root),"PIF_ENGINE_ROOT":str(root / "module"),"PIF_ENGINE_SCRIPTS":str(SCRIPTS)},capture_output=True)
        assert rejected.returncode != 0, invalid
        assert not (root / "mutated").exists(), invalid
    run('''
ui_print() { :; }
. "$PIF_ENGINE_SCRIPTS/path_rules.sh"
pif_valid_app_path /system_ext/app/helluvaProductDevice*
pif_valid_app_path /product/app/XiaomiEUInject
''', root)
    # Old/corrupt manifests must never clean the root of the module's system overlay.
    (root / "module/system").mkdir(parents=True)
    (root / "module/system/.replace").touch()
    (root / "overlay-paths.txt").write_text("system/\nsystem/app/../\n")
    run('sh "$PIF_ENGINE_SCRIPTS/stage.sh" cleanup',root,PIF_ENGINE_MODE="full")
    assert (root / "module/system/.replace").exists()

    # Preserve unowned contents and the manifest when an overlay cannot be removed.
    retained=root / "module/system/app/TestApp"
    retained.mkdir(parents=True)
    (retained / ".replace").touch()
    (retained / "external").write_text("keep")
    (root / "overlay-paths.txt").write_text("system/app/TestApp\n")
    attempted=subprocess.run(["sh",str(SCRIPTS / "stage.sh"),"cleanup"],env={**os.environ,"PIF_ENGINE_DATA":str(root),"PIF_ENGINE_ROOT":str(root / "module"),"PIF_ENGINE_SCRIPTS":str(SCRIPTS),"PIF_ENGINE_MODE":"full"},capture_output=True)
    assert attempted.returncode != 0
    assert (retained / "external").read_text()=="keep"
    assert (root / "overlay-paths.txt").exists()
    (retained / "external").unlink()
    run('sh "$PIF_ENGINE_SCRIPTS/stage.sh" cleanup',root,PIF_ENGINE_MODE="full")
    assert not (root / "overlay-paths.txt").exists()

    for mode in ["full", "scripts-only"]:
        (root / "skippersistprop").touch()
        (root / "custom.app_replace_list.txt").write_text("")
        run('''
getprop() { printf '1'; }
magisk() { printf 'magisk %s\\n' "$*" >> "$PIF_ENGINE_DATA/order"; return 0; }
resetprop() {
  if [ "$#" = 1 ]; then printf 'original';
  else printf 'resetprop %s\\n' "$*" >> "$PIF_ENGINE_DATA/order";
  fi
}
toybox() { printf '1'; }
ui_print() { :; }
KSU=false APATCH=false
. "$PIF_ENGINE_SCRIPTS/post-fs-data.sh"
printf 'late\\n' >> "$PIF_ENGINE_DATA/order"
. "$PIF_ENGINE_SCRIPTS/service.sh"
''', root, PIF_ENGINE_MODE=mode)
        lines = (root / "order").read_text().splitlines()
        late = lines.index("late")
        assert any("ro.debuggable" in item for item in lines[:late])
        assert not any("ro.boot.verifiedbootstate" in item for item in lines[:late])
        assert any("ro.boot.verifiedbootstate" in item for item in lines[late:])
        assert not any("magisk --denylist" in item for item in lines)
        (root / "order").unlink()

    (root / "disabled").write_text("stop")
    run('''
getprop() { printf '1'; }
resetprop() { printf 'unexpected' >> "$PIF_ENGINE_DATA/after-stop"; }
ui_print() { :; }
. "$PIF_ENGINE_SCRIPTS/post-fs-data.sh"
. "$PIF_ENGINE_SCRIPTS/service.sh"
''', root, PIF_ENGINE_MODE="full")
    assert not (root / "after-stop").exists()
    (root / "enabled-token").write_text("stop")
    run('''
. "$PIF_ENGINE_SCRIPTS/power.sh"
pif_running
''',root)
    (root / "disabled").write_text("")
    (root / "enabled-token").write_text("")
    run('''
. "$PIF_ENGINE_SCRIPTS/power.sh"
if pif_running; then exit 1; fi
''',root)

    (root / "disabled").unlink()
    (root / "enabled-token").unlink()
    (root / "module").mkdir(exist_ok=True)
    for flag in ["disable", "remove"]:
        path = root / "module" / flag
        path.touch()
        run('''
. "$PIF_ENGINE_SCRIPTS/power.sh"
if pif_running; then exit 1; fi
''', root)
        path.unlink()


print("Shell restoration, shutdown gate, ROM path guards and OEM ordering passed")
