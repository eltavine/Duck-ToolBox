"""Payload contracts shared by packaging and the release inventory."""

import json
import re

FULL_ID = "duck-toolbox"
LITE_ID = "duck-toolbox-lite"
ARTIFACT_PREFIX = {FULL_ID: FULL_ID, LITE_ID: "duck-lite"}
LITE_SOURCES = (
    "LICENSE",
    "action.sh",
    "bin/duckctl.sh",
    "boot-completed.sh",
    "customize.sh",
    "hotinstall.sh",
    "identity.sh",
    "late-load.sh",
    "licenses/rkp-v1-oracle.txt",
    "post-fs-data.sh",
    "prop.sh",
    "service.sh",
    "uninstall.sh",
    "util_functions.sh",
)
LITE_REQUIRED = set(LITE_SOURCES) | {
    "bin/duckd",
    "module.prop",
    "webroot/build.json",
    "webroot/config.json",
    "webroot/duck-logo.svg",
    "webroot/index.html",
}
LITE_WEBUI = {"edition": "lite", "features": ["rkp", "rkp-device", "tricky-store"]}


def validate_lite_payload(names, read, metadata):
    missing = LITE_REQUIRED - names
    unexpected = {
        name for name in names - LITE_REQUIRED
        if not re.fullmatch(r"webroot/assets/[A-Za-z0-9_.-]+\.(?:css|js|woff2?)", name)
    }
    if missing or unexpected:
        raise ValueError(f"Invalid Lite payload: missing={sorted(missing)}, unexpected={sorted(unexpected)}")
    if metadata.get("id") != LITE_ID or "updateJson" in metadata:
        raise ValueError("Lite must have its own module ID and use manual updates")
    if json.loads(read("webroot/build.json")) != LITE_WEBUI:
        raise ValueError("Invalid Lite WebUI feature catalog")
    if not any(name.startswith("webroot/assets/") and name.endswith(".js") for name in names):
        raise ValueError("Missing Lite WebUI JavaScript")
    binary = read("bin/duckd")
    if not binary.startswith(b"\x7fELF\x02\x01") or binary[18:20] != b"\xb7\x00":
        raise ValueError("Lite requires an Android ARM64 ELF backend")
    if b"Duck ToolBox Lite backend." not in binary:
        raise ValueError("Lite backend was not built with the Lite feature set")


def module_properties(body):
    return dict(line.split("=", 1) for line in body.splitlines() if "=" in line)
