"""Small source and install-archive fixtures for supply-chain contract tests."""

import hashlib
import json
import zipfile

from inputs import LOCKFILES, digest
from payload import ARTIFACT_PREFIX, FULL_ID, LITE_ID, LITE_REQUIRED, LITE_WEBUI

COMMIT = "a" * 40


def write(root, name, content):
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)
    return path


def source(root):
    for index, relative in enumerate(LOCKFILES[:4]):
        write(root, relative, f'[[package]]\nname="dep-{index}"\nversion="1.0.0"\n')
    write(
        root,
        LOCKFILES[4],
        "---\nlockfileVersion: '9.0'\npackages:\n  pnpm@12.16.1: {}\n---\nlockfileVersion: '9.0'\npackages:\n  '@scope/app@1.0.0(peer@2.0.0)': {}\n",
    )
    write(
        root,
        LOCKFILES[5],
        "group:java-lib:2.0.0=releaseRuntimeClasspath\nempty=debug\n",
    )
    write(root, "Cargo.toml", "[workspace]\n")
    write(root, "ui/package.json", '{"name":"webui"}')
    for name in ["play-integrity", "device-faker"]:
        write(
            root,
            f"components/{name}/UPSTREAM.json",
            json.dumps(
                {
                    "repository": f"https://github.com/example/{name}",
                    "revision": "b" * 40,
                    "license": "GPL-3.0-only",
                }
            ),
        )
    write(
        root,
        "components/linux-env/tini.json",
        json.dumps(
            {
                "version": "0.19.0",
                "asset": "https://example.com/tini",
                "license": "MIT",
                "sha256": hashlib.sha256(b"tini").hexdigest(),
            }
        ),
    )
    write(
        root,
        "scripts/build-play-integrity.py",
        'DEPENDENCIES = {"json": ("example/json", "' + "c" * 40 + '")}\n',
    )
    write(root, "CHANGELOG.md", "Release changes\n")
    return root


def inventory(root, lockfiles=LOCKFILES):
    from inputs import expected_packages

    packages, files, relationships = [], [], []
    for index, (relative, entries) in enumerate(expected_packages(root, lockfiles).items()):
        file_id = f"SPDXRef-Input-{index}"
        files.append(
            {
                "SPDXID": file_id,
                "fileName": relative,
                "checksums": [
                    {
                        "algorithm": "SHA1",
                        "checksumValue": hashlib.sha1(
                            (root / relative).read_bytes()
                        ).hexdigest(),
                    },
                    {"algorithm": "SHA256", "checksumValue": digest(root / relative)},
                ],
                "licenseConcluded": "NOASSERTION",
                "copyrightText": "NOASSERTION",
            }
        )
        for number, (name, version) in enumerate(sorted(entries)):
            package_id = f"SPDXRef-Package-{index}-{number}"
            maven = relative == LOCKFILES[5]
            packages.append(
                {
                    "SPDXID": package_id,
                    "name": name.split(":", 1)[1] if maven else name,
                    "versionInfo": version,
                    "downloadLocation": "NOASSERTION",
                    "filesAnalyzed": False,
                    "licenseDeclared": "NOASSERTION",
                    "licenseConcluded": "NOASSERTION",
                    "copyrightText": "NOASSERTION",
                    "externalRefs": (
                        [
                            {
                                "referenceCategory": "PACKAGE-MANAGER",
                                "referenceType": "purl",
                                "referenceLocator": "pkg:maven/"
                                + name.replace(":", "/", 1)
                                + "@"
                                + version,
                            }
                        ]
                        if maven
                        else []
                    ),
                }
            )
            relationships.append(
                {
                    "spdxElementId": package_id,
                    "relatedSpdxElement": file_id,
                    "relationshipType": "OTHER",
                    "comment": "evident-by: lockfile",
                }
            )
    return {
        "spdxVersion": "SPDX-2.3",
        "SPDXID": "SPDXRef-DOCUMENT",
        "dataLicense": "CC0-1.0",
        "name": "build-inputs",
        "documentNamespace": "https://example.com/sbom/test",
        "creationInfo": {
            "created": "2026-11-02T00:00:00Z",
            "creators": ["Tool: syft-1.55.0"],
        },
        "packages": packages,
        "files": files,
        "relationships": relationships,
    }


def archive(root, version="1.2.3", variant="full", version_code="123"):
    root.mkdir(parents=True, exist_ok=True)
    module_id = LITE_ID if variant == "lite" else FULL_ID
    path = root / f"{ARTIFACT_PREFIX[module_id]}-{version}.zip"
    with zipfile.ZipFile(path, "w") as output:
        output.writestr("module.prop", f"id={module_id}\nversion={version}\nversionCode={version_code}\n")
        if variant == "lite":
            files = {
                "webroot/build.json": json.dumps(LITE_WEBUI),
                "bin/duckd": b"\x7fELF\x02\x01" + bytes(12) + b"\xb7\x00Duck ToolBox Lite backend.",
            }
            for name in LITE_REQUIRED - {"module.prop"} | {"webroot/assets/app.js"}:
                output.writestr(name, files.get(name, b"fixture payload"))
            return path
        output.writestr("bin/tini-arm64", b"tini")
        for name in ["play-integrity", "device-faker"]:
            output.writestr(f"{name}/build.json", json.dumps({"source_commit": COMMIT}))
        for name in [
            "bin/duckd",
            "webroot/index.html",
            "zygisk/arm64-v8a.so",
            "zygisk/armeabi-v7a.so",
            "play-integrity/pif-engine",
            "play-integrity/classes.dex",
            "device-faker/device-faker-engine",
        ]:
            output.writestr(name, b"fixture payload")
    return path
