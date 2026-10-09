"""Create a versioned release commit/tag without pushing a commit back to main."""
import os
from pathlib import Path
import re
import subprocess
import tomllib


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def version_tuple(value):
    return tuple(map(int, value.split("."))) if re.fullmatch(r"\d+\.\d+\.\d+", value) else None


def next_version(package_version, tags):
    current = version_tuple(package_version)
    if current is None:
        raise ValueError("Package version must be a stable major.minor.patch version")
    versions = [v for tag in tags if tag.startswith("v") and (v := version_tuple(tag[1:])) is not None]
    if not versions or current > max(versions):
        return package_version
    major, minor, patch = max(versions)
    return f"{major}.{minor}.{patch + 1}"


def output(tag):
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
        stream.write(f"tag={tag}\nref={git('rev-parse', tag + '^{commit}')}\n")


def main():
    package = tomllib.loads(Path("Cargo.toml").read_text(encoding="utf-8"))["package"]
    ref = os.environ["GITHUB_REF"]
    if ref.startswith("refs/tags/"):
        tag = ref.removeprefix("refs/tags/")
        if tag != "v" + package["version"]:
            raise ValueError("Tag and package version differ")
        output(tag)
        return
    if ref != "refs/heads/main":
        raise ValueError("Automatic releases only accept main")
    source = git("rev-parse", "HEAD")
    tags = git("tag", "--list", "v*").splitlines()
    # A rerun reuses its release commit, even if newer pushes released meanwhile.
    for tag in tags:
        if version_tuple(tag[1:]) is not None and f"Release-Source: {source}" in git("log", "-1", "--format=%B", tag):
            output(tag)
            return
    version = next_version(package["version"], tags)
    manifest = Path("Cargo.toml")
    text = manifest.read_text(encoding="utf-8")
    text, count = re.subn(r'(?m)^version = "[^"]+"', f'version = "{version}"', text, count=1)
    if count != 1:
        raise ValueError("Package version missing")
    manifest.write_text(text, encoding="utf-8", newline="\n")
    lock = Path("Cargo.lock")
    text = lock.read_text(encoding="utf-8")
    pattern = r'(\[\[package\]\]\nname = "' + re.escape(package["name"]) + r'"\nversion = ")[^"]+"'
    text, count = re.subn(pattern, lambda m: m[1] + version + '"', text)
    if count != 1:
        raise ValueError("Root package missing or duplicated in Cargo.lock")
    lock.write_text(text, encoding="utf-8", newline="\n")
    git("config", "user.name", "github-actions[bot]")
    git("config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")
    git("add", "Cargo.toml", "Cargo.lock")
    git("commit", "--allow-empty", "-m", f"Release v{version}\n\nRelease-Source: {source}")
    tag = "v" + version
    git("tag", tag)
    # GITHUB_TOKEN-created tags do not recursively trigger the tag workflow.
    git("push", "origin", f"refs/tags/{tag}")
    output(tag)


if __name__ == "__main__":
    main()
