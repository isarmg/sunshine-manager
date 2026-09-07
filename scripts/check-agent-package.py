#!/usr/bin/env python3
"""Independently unpack/verify an Agent package; native service tests only on disposable CI."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import tarfile
import tempfile
import time
import zipfile


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def powershell(script):
    return run("powershell.exe", "-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; " + script)


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def verify(archive, destination, sha):
    windows = archive.suffix == ".zip"
    suffix = ".zip" if windows else ".tar.gz"
    name = archive.name.removesuffix(suffix)
    if not re.fullmatch(r"sunshine-agent-[0-9]+\.[0-9]+\.[0-9]+(?:-rc\.[0-9]+)?-x86_64-(?:pc-windows-msvc|unknown-linux-gnu)", name):
        raise ValueError("unexpected archive name")
    checksum = archive.with_name(archive.name + ".sha256").read_text().strip()
    if checksum != f"{digest(archive)}  {archive.name}":
        raise ValueError("archive digest mismatch")
    allowed = {"sunshine-agent.exe" if windows else "sunshine-agent", "README.md", "LICENSE", "manifest.json", "SHA256SUMS", "bootstrap.example.json"}
    allowed.update(["install-windows.ps1", "uninstall-windows.ps1"] if windows else ["install-linux.sh", "uninstall-linux.sh", "sunshine-agent.service"])
    root = destination / name
    root.mkdir()
    seen = set()
    budget = 128 * 1024 * 1024

    def write(path, size, data):
        nonlocal budget
        parts = path.split("/")
        if len(parts) != 2 or parts[0] != name or parts[1] not in allowed or parts[1] in seen:
            raise ValueError("unexpected or duplicate archive entry")
        budget -= size
        if size < 0 or budget < 0:
            raise ValueError("archive budget exceeded")
        payload = data.read(size + 1)
        if len(payload) != size:
            raise ValueError("archive entry size mismatch")
        with (root / parts[1]).open("xb") as output:
            output.write(payload)
        seen.add(parts[1])

    if windows:
        with zipfile.ZipFile(archive) as source:
            for entry in source.infolist():
                if entry.is_dir() or (entry.external_attr >> 16) & 0o170000 not in (0, 0o100000):
                    raise ValueError("non-file ZIP entry")
                with source.open(entry) as data:
                    write(entry.filename, entry.file_size, data)
    else:
        with tarfile.open(archive) as source:
            for entry in source:
                if entry.isdir() and entry.name == name:
                    continue
                if not entry.isfile():
                    raise ValueError("non-file TAR entry")
                with source.extractfile(entry) as data:
                    write(entry.name, entry.size, data)
    if seen != allowed:
        raise ValueError("incomplete package")
    manifest = json.loads((root / "manifest.json").read_text())
    target = "x86_64-pc-windows-msvc" if windows else "x86_64-unknown-linux-gnu"
    if manifest["source_commit"] != sha or manifest["target"] != target or manifest["protocol"] != "sunshine-management/1" or manifest["product"] != "sunshine-agent":
        raise ValueError("package identity mismatch")
    if name != f"sunshine-agent-{manifest['version']}-{target}" or manifest["authenticode_signed"] is not False:
        raise ValueError("package version or signature declaration mismatch")
    files = allowed - {"manifest.json", "SHA256SUMS"}
    if manifest["files"] != {p: digest(root / p) for p in sorted(files)}:
        raise ValueError("manifest digest mismatch")
    expected_checksums = "".join(f"{digest(root / p)}  {p}\n" for p in sorted(allowed - {"SHA256SUMS"}))
    if (root / "SHA256SUMS").read_text() != expected_checksums:
        raise ValueError("file checksum mismatch")
    binary = root / ("sunshine-agent.exe" if windows else "sunshine-agent")
    binary.chmod(0o755)
    if run(str(binary), "--version") != f"sunshine-agent {manifest['version']} (git {sha}; sunshine-management/1)":
        raise ValueError("executable identity mismatch")
    return root, binary


def install_test(root, binary, temporary):
    # This test intentionally creates a real service/account, but never on a user's host.
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        raise ValueError("--install is restricted to disposable GitHub-hosted runners")
    windows = platform.system() == "Windows"
    ca = temporary / "ca.pem"
    subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", str(temporary / "key.pem"), "-out", str(ca), "-days", "1", "-subj", "/CN=Agent installation test", "-addext", "basicConstraints=critical,CA:TRUE", "-addext", "subjectAltName=IP:127.0.0.1"], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    bootstrap = temporary / "bootstrap.json"
    bootstrap.write_text(json.dumps({"manager_endpoint": "wss://127.0.0.1:9/sunshine-agent/v1/connect", "manager_ca_pem": ca.read_text(), "manager_id": "11111111-1111-4111-8111-111111111111", "device_id": "22222222-2222-4222-8222-222222222222", "enrollment_token": "a" * 64, "sunshine_endpoint": "https://127.0.0.1:47990/", "sunshine_ca_pem": ca.read_text(), "sunshine_username": "test", "sunshine_password": "installation-fixture-only", "restart_allowed": False}), encoding="utf-8")
    bootstrap.chmod(0o600)
    if windows:
        # Protect only this newly created fixture, using numeric SIDs (localized Windows safe).
        escaped = str(temporary).replace("'", "''")
        powershell(f"$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value; & icacls.exe '{escaped}' /inheritance:r /grant:r ('*'+$sid+':(OI)(CI)F') '*S-1-5-18:(OI)(CI)F' '*S-1-5-32-544:(OI)(CI)F' /T /Q; if($LASTEXITCODE){{throw 'fixture ACL failed'}}")
        install = ["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(root / "install-windows.ps1"), "-Binary", str(binary), "-Bootstrap", str(bootstrap)]
        subprocess.run(install, check=True)
        if subprocess.run(install, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
            raise ValueError("installer overwrote an existing installation")
        powershell("Restart-Service SunshineAgent; (Get-Service SunshineAgent).WaitForStatus('Running',[TimeSpan]::FromSeconds(30))")
        time.sleep(3)
        if powershell("(Get-Service SunshineAgent).Status") != "Running":
            raise ValueError("Agent failed to remain running while Manager was offline")
        subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", str(root / "uninstall-windows.ps1")], check=True)
        if not (Path(os.environ["ProgramData"]) / "SunshineAgent/provisioning/state.sqlite3").is_file():
            raise ValueError("uninstall removed protected state")
    else:
        install = ["bash", str(root / "install-linux.sh"), str(binary), str(bootstrap)]
        subprocess.run(install, check=True)
        if subprocess.run(install, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
            raise ValueError("installer overwrote an existing installation")
        subprocess.run(["systemctl", "restart", "sunshine-agent.service"], check=True)
        time.sleep(3)
        subprocess.run(["systemctl", "is-active", "--quiet", "sunshine-agent.service"], check=True)
        state = Path("/var/lib/sunshine-agent/provisioning")
        if (state.stat().st_mode & 0o077) != 0 or not (state / "identity.json").is_file():
            raise ValueError("protected persistent identity missing")
        subprocess.run(["bash", str(root / "uninstall-linux.sh")], check=True)
        if not (state / "identity.json").is_file():
            raise ValueError("uninstall removed protected state")
    print("Native install, autostart configuration, restart, overwrite refusal and state-preserving uninstall passed; no real Sunshine was modified.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--sha", required=True)
    parser.add_argument("--install", action="store_true")
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{40}", args.sha):
        parser.error("full source SHA required")
    with tempfile.TemporaryDirectory(prefix="agent-check-") as tmp:
        temporary = Path(tmp)
        root, binary = verify(args.archive.resolve(), temporary, args.sha)
        print("Independent archive, manifest, checksums and executable identity verified.")
        if args.install:
            install_test(root, binary, temporary)


if __name__ == "__main__":
    main()
