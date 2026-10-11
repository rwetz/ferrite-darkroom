"""Package an already-built release folder, without compiling the app again."""
import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import tomllib


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--output-dir", type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    manifest = tomllib.loads((repo / "Cargo.toml").read_text(encoding="utf-8"))
    binary = manifest.get("bin", [{"name": manifest["package"]["name"]}])[0]["name"]
    if not re.fullmatch(r"[a-z][a-z0-9-]*", binary):
        raise ValueError("invalid binary name")
    if not re.fullmatch(r"v\d+\.\d+\.\d+", args.tag):
        raise ValueError("expected a vMAJOR.MINOR.PATCH tag")
    source = args.source.resolve(strict=True)
    for required in (f"{binary}.exe", "LICENSE", "NOTICE", "README.md", "CHANGELOG.md"):
        if not (source / required).is_file():
            raise FileNotFoundError(source / required)
    product = " ".join(word.capitalize() for word in binary.removeprefix("ferrite-").split("-"))
    compiler = shutil.which("makensis") or os.environ.get("MAKENSIS")
    if not compiler:
        candidate = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "NSIS/makensis.exe"
        if candidate.is_file():
            compiler = str(candidate)
    if not compiler:
        raise RuntimeError("Install NSIS or set MAKENSIS to makensis.exe")
    output_dir = (args.output_dir or source.parent).resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    output = output_dir / f"{binary}-{args.tag}-x86_64-pc-windows-msvc-setup.exe"
    definitions = dict(PRODUCT=product, BINARY=binary, VERSION=args.tag[1:],
                       SOURCE=source, OUTPUT=output, ICON=repo / "assets/logo.ico")
    prefix = "/D" if os.name == "nt" else "-D"
    subprocess.run([compiler, *(f"{prefix}{key}={value}" for key, value in definitions.items()),
                    str(Path(__file__).with_name("windows-installer.nsi"))], check=True)
    print(output)


if __name__ == "__main__":
    main()
