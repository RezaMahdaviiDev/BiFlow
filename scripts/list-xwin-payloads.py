#!/usr/bin/env python3
"""Emit cargo-xwin 0.19.2 / xwin 0.6.6 download cache names from VisualStudio.vsman.

xwin stores CRT vsix files under their Microsoft fileName and SDK/UCRT MSI files
under renamed names (ucrt.msi, Win11SDK_*_headers.msi, …). Prefetch those exact
paths with curl so cargo-xwin's ureq client never has to fetch the large bodies.
"""

from __future__ import annotations

import json
import sys
from typing import Any, Iterable


def parse_version(value: str) -> tuple[int, ...]:
    parts: list[int] = []
    for part in value.split("."):
        if not part.isdigit():
            break
        parts.append(int(part))
    return tuple(parts)


def payload_map(item: dict[str, Any]) -> list[dict[str, Any]]:
    return list(item.get("payloads") or [])


def find_payload(item: dict[str, Any], predicate) -> dict[str, Any] | None:
    for payload in payload_map(item):
        name = payload.get("fileName") or ""
        if predicate(name):
            return payload
    return None


def emit(filename: str, payload: dict[str, Any]) -> dict[str, str]:
    return {
        "sha256": str(payload["sha256"]).lower(),
        "filename": filename,
        "url": payload["url"],
        "size": str(payload.get("size") or ""),
    }


def latest_crt_version(packages: dict[str, dict[str, Any]]) -> str:
    build_tools = packages.get("Microsoft.VisualStudio.Product.BuildTools")
    if not build_tools:
        raise SystemExit("unable to find Microsoft.VisualStudio.Product.BuildTools")
    versions: list[str] = []
    for key in (build_tools.get("dependencies") or {}):
        prefix = "Microsoft.VisualStudio.Component.VC."
        suffix = ".x86.x64"
        if key.startswith(prefix) and key.endswith(suffix):
            versions.append(key[len(prefix) : -len(suffix)])
    if not versions:
        raise SystemExit("unable to find a CRT version in BuildTools")
    return max(versions, key=parse_version)


def latest_sdk_id(packages: dict[str, dict[str, Any]]) -> str:
    found: list[tuple[int, tuple[int, ...], str]] = []
    for key in packages:
        if not key.startswith("Win") or "SDK_" not in key:
            continue
        try:
            major_s, version_s = key[3:].split("SDK_", 1)
            major = int(major_s)
        except ValueError:
            continue
        found.append((major, parse_version(version_s), key))
    if not found:
        raise SystemExit("unable to find a WinSDK package")
    return max(found)[2]


def crt_payloads(
    packages: dict[str, dict[str, Any]], crt_version: str, arch_ms: str
) -> Iterable[dict[str, str]]:
    header_key = f"Microsoft.VC.{crt_version}.CRT.Headers.base"
    headers = packages.get(header_key)
    if not headers or not payload_map(headers):
        raise SystemExit(f"unable to find CRT headers '{header_key}'")
    yield emit(payload_map(headers)[0]["fileName"], payload_map(headers)[0])
    for variant in ("Desktop", "Store"):
        lib_key = f"Microsoft.VC.{crt_version}.CRT.{arch_ms}.{variant}.base"
        libs = packages.get(lib_key)
        if not libs or not payload_map(libs):
            raise SystemExit(f"unable to find CRT libs '{lib_key}'")
        yield emit(payload_map(libs)[0]["fileName"], payload_map(libs)[0])


def sdk_payloads(
    packages: dict[str, dict[str, Any]], sdk_id: str, arch_ms: str, arch_name: str
) -> Iterable[dict[str, str]]:
    sdk = packages.get(sdk_id)
    if not sdk:
        raise SystemExit(f"unable to locate SDK '{sdk_id}'")

    required = [
        (
            f"{sdk_id}_headers.msi",
            lambda name: name.endswith("Windows SDK Desktop Headers x86-x86_en-us.msi"),
        ),
        (
            f"{sdk_id}_store_headers.msi",
            lambda name: name.endswith(
                "Windows SDK for Windows Store Apps Headers-x86_en-us.msi"
            ),
        ),
        (
            f"{sdk_id}_{arch_ms}_headers.msi",
            lambda name, arch=arch_ms: name.endswith(
                f"Windows SDK Desktop Headers {arch}-x86_en-us.msi"
            ),
        ),
        (
            f"{sdk_id}_libs_{arch_name}.msi",
            lambda name, arch=arch_ms: name.endswith(
                f"Windows SDK Desktop Libs {arch}-x86_en-us.msi"
            ),
        ),
        (
            f"{sdk_id}_store_libs.msi",
            lambda name: name.endswith(
                "Windows SDK for Windows Store Apps Libs-x86_en-us.msi"
            ),
        ),
    ]
    for filename, predicate in required:
        payload = find_payload(sdk, predicate)
        if payload is None:
            raise SystemExit(f"unable to find SDK payload for {filename}")
        yield emit(filename, payload)

    optional = [
        (
            f"{sdk_id}_uap_headers.msi",
            lambda name: name.endswith("Windows SDK OnecoreUap Headers x86-x86_en-us.msi"),
        ),
        (
            f"{sdk_id}_store_headers_onecoreuap.msi",
            lambda name: name.endswith(
                "Windows SDK for Windows Store Apps Headers OnecoreUap-x86_en-us.msi"
            ),
        ),
    ]
    for filename, predicate in optional:
        payload = find_payload(sdk, predicate)
        if payload is not None:
            yield emit(filename, payload)


def ucrt_payloads(packages: dict[str, dict[str, Any]]) -> Iterable[dict[str, str]]:
    ucrt = packages.get("Microsoft.Windows.UniversalCRT.HeadersLibsSources.Msi")
    if not ucrt:
        raise SystemExit("unable to find Universal CRT")
    msi = find_payload(
        ucrt,
        lambda name: name == "Universal CRT Headers Libraries and Sources-x86_en-us.msi",
    )
    if msi is None:
        raise SystemExit("unable to find Universal CRT MSI")
    yield emit("ucrt.msi", msi)
    for payload in payload_map(ucrt):
        name = payload.get("fileName") or ""
        if not name.endswith(".cab"):
            continue
        cab = name.split("\\")[-1]
        yield emit(f"ucrt/{cab}", payload)


def list_payloads(manifest: dict[str, Any], arch: str) -> list[dict[str, str]]:
    packages = {item["id"]: item for item in manifest.get("packages") or []}
    arch_ms = {"x86_64": "x64", "x86": "x86", "aarch64": "ARM64", "aarch": "arm"}[arch]
    rows = []
    rows.extend(crt_payloads(packages, latest_crt_version(packages), arch_ms))
    rows.extend(sdk_payloads(packages, latest_sdk_id(packages), arch_ms, arch))
    rows.extend(ucrt_payloads(packages))
    return rows


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print("usage: list-xwin-payloads.py <vsman> [arch]", file=sys.stderr)
        return 2
    arch = argv[2] if len(argv) > 2 else "x86_64"
    with open(argv[1], encoding="utf-8") as handle:
        manifest = json.load(handle)
    for row in list_payloads(manifest, arch):
        print(f"{row['sha256']}\t{row['filename']}\t{row['url']}\t{row['size']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
