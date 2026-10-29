#!/usr/bin/env python3
"""Exercise Linux environments in an explicitly selected, disposable rooted Android VM."""
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]


class Device:
    def __init__(self, args):
        self.adb = [args.adb, "-s", args.serial]
        self.base = "/data/local/tmp/duck-linux-test-" + uuid.uuid4().hex
        self.command = ["env", "DUCK_TOOLBOX_ROOT=" + self.base + "/module",
                        "DUCK_TOOLBOX_DATA_ROOT=" + self.base + "/data",
                        self.base + "/module/bin/duckd", "linux-env"]

    def shell(self, words, text=None, check=True):
        return subprocess.run(self.adb + ["shell", shlex.join(words)], input=text,
                              text=True, capture_output=True, check=check, timeout=45)

    def push(self, source, target):
        subprocess.run(self.adb + ["push", str(source), target], check=True, capture_output=True, timeout=60)

    def api(self, args, data=None, error=None):
        output = self.shell(self.command + args, json.dumps(data) if data is not None else None, check=False)
        response = json.loads(output.stdout)
        if error:
            assert not response["ok"] and response["error"]["code"] == error, response
        else:
            assert output.returncode == 0 and response["ok"], response
        return response["data"]

    def status(self):
        return self.api(["status"])

    def wait(self, predicate, timeout=40):
        until = time.monotonic() + timeout
        while True:
            state = self.status()
            if predicate(state):
                return state
            assert time.monotonic() < until, state
            time.sleep(0.15)

    def target(self):
        view = self.status()["environments"][0]
        return {"id": view["environment"]["id"], "revision": view["revision"], "confirmed": True}

    def terminal(self, text):
        output = self.shell(self.command + ["shell", "sample"], text + "\nexit\n")
        assert output.returncode == 0, output
        return output.stdout


def run(args):
    digest = hashlib.sha256(args.rootfs.read_bytes()).hexdigest()
    if args.sha256:
        assert digest == args.sha256, "Rootfs digest differs from --sha256"
    device = Device(args)
    # Never silently choose a USB device or change its SELinux mode.
    assert device.shell(["id", "-u"]).stdout.strip() == "0", "adb must already run as ROOT"
    device.shell(["mkdir", "-p", device.base + "/module/bin"])
    completed = False
    try:
        device.push(args.binary, device.base + "/module/bin/duckd")
        device.push(ROOT / "module/bin/tini-arm64", device.base + "/module/bin/tini-arm64")
        device.push(ROOT / "module/module.prop", device.base + "/module/module.prop")
        device.push(args.rootfs, device.base + "/root.tar.gz")
        device.shell(["chmod", "755", device.base + "/module/bin/duckd", device.base + "/module/bin/tini-arm64"])
        inspected = device.api(["inspect", device.base + "/root.tar.gz"])
        assert inspected["sha256"] == digest
        config = {"name": "Test environment", "auto_start": False, "dns": ["1.1.1.1"], "services": []}
        request = {"id": "sample", "config": config, "path": inspected["path"], "sha256": digest, "confirmed": False}
        device.api(["import", "--stdin-json"], request, error="confirmation_required")
        request["confirmed"] = True
        device.api(["import", "--stdin-json"], request)
        state = device.wait(lambda s: s["job"]["phase"] in ("done", "failed"))
        assert state["job"]["phase"] == "done", state
        directory = device.base + "/data/var/linux-env/sample"
        assert device.shell(["stat", "-c", "%a", directory]).stdout.strip() == "700"
        stale = device.target()
        config["services"] = [{"id": "worker", "command": "echo service-ready; while :; do /bin/sleep 1; done", "enabled": True}]
        device.api(["save", "--stdin-json"], {"target": stale, "config": config})
        device.api(["start", "--stdin-json"], stale, error="state_changed")
        target = device.target()
        device.api(["start", "--stdin-json"], target)
        state = device.wait(lambda s: s["environments"][0]["phase"] in ("running", "failed"))
        assert state["environments"][0]["phase"] == "running", state
        assert "nameserver 1.1.1.1" in device.terminal("/bin/cat /etc/resolv.conf")
        assert "terminal-ok" in device.terminal("echo terminal-ok")
        assert "service-ready" in device.api(["logs", "--id", "sample"])
        device.api(["remove", "--stdin-json"], target, error="operation_busy")
        init = state["environments"][0]["runtime"]["init"]["pid"]
        device.api(["stop", "--stdin-json"], target)
        assert device.status()["environments"][0]["phase"] == "stopped"
        assert device.shell(["test", "-e", f"/proc/{init}"], check=False).returncode != 0
        # Simulate supervisor loss. cgroup fallback must report remaining descendants and stop them.
        device.api(["start", "--stdin-json"], target)
        state = device.wait(lambda s: s["environments"][0]["phase"] == "running")
        runtime = state["environments"][0]["runtime"]
        device.shell(["kill", "-KILL", str(runtime["supervisor"]["pid"])])
        state = device.wait(lambda s: s["environments"][0]["phase"] in ("unknown", "failed"))
        if state["environments"][0]["phase"] == "unknown":
            device.api(["remove", "--stdin-json"], target, error="operation_busy")
            device.shell(["rm", directory + "/runtime.json"])
            assert device.status()["environments"][0]["phase"] == "unknown"
            device.api(["stop", "--stdin-json"], target)
        # Boot replay is limited to one attempt per environment instance per Android boot.
        config["auto_start"] = True
        device.api(["save", "--stdin-json"], {"target": target, "config": config})
        target = device.target()
        assert device.api(["boot-apply"])[0]["code"] == "starting"
        device.wait(lambda s: s["environments"][0]["phase"] == "running")
        device.api(["stop", "--stdin-json"], target)
        assert device.api(["boot-apply"])[0]["code"] == "already_attempted"
        device.api(["remove", "--stdin-json"], target)
        assert device.status()["environments"] == []
        completed = True
        print("Passed: import, private permissions, stale revision, services, DNS, terminal, stop, supervisor loss, boot replay and removal.")
        print("Kernel:", device.shell(["uname", "-r"]).stdout.strip())
        print("SELinux:", device.shell(["getenforce"]).stdout.strip())
        print("Rootfs SHA-256:", digest)
        print("PID namespace:", runtime["pid_namespace"])
    finally:
        cleanup = device.shell(device.command + ["stop-all"], check=False)
        try:
            stopped = json.loads(cleanup.stdout)["ok"]
        except (ValueError, KeyError):
            stopped = False
        if stopped:
            device.shell(["rm", "-rf", device.base])
        else:
            print("Cleanup could not be confirmed; preserved", device.base)
        if not completed:
            print("Device integration checks did not complete.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True, help="Explicit disposable rooted Android VM serial")
    parser.add_argument("--adb", default="adb")
    parser.add_argument("--binary", type=Path, default=ROOT / "target/aarch64-linux-android/debug/duckd")
    parser.add_argument("--rootfs", type=Path, required=True, help="Trusted ARM64 tar or tar.gz rootfs")
    parser.add_argument("--sha256", help="Expected archive SHA-256")
    run(parser.parse_args())


if __name__ == "__main__":
    main()
