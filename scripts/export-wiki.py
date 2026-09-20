#!/usr/bin/env python3
"""Validate reviewed Markdown and export it to GitHub Wiki's flat page names.

Only inline Markdown links are used by these guides. Keep GitHub's renderer;
this adapter does not render Markdown, contact GitHub, or push a repository.
"""

import argparse
from pathlib import Path
import re
import subprocess
from urllib.parse import quote, urlsplit


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "docs/wiki"
REPOSITORY = "https://github.com/iebb/termgram"
LINK = re.compile(r"\]\(([^\s)]+)\)")


def pages():
    english = {path.name for path in (SOURCE / "en").glob("*.md")}
    chinese = {path.name for path in (SOURCE / "zh-CN").glob("*.md")}
    if not english or english != chinese:
        raise ValueError(f"English/Chinese page mismatch: {english ^ chinese}")
    paths = [SOURCE / "Home.md"] + sorted(SOURCE.glob("*/*.md"))
    return {
        path: "-".join(path.relative_to(SOURCE).with_suffix("").parts)
        for path in paths
    }


def rewrite(text, source, names, revision):
    def link(match):
        target = match[1]
        if urlsplit(target).scheme or target.startswith(("#", "//")):
            return match[0]
        path, separator, anchor = target.partition("#")
        resolved = (source.parent / path).resolve()
        if not resolved.is_relative_to(ROOT) or not resolved.exists():
            raise ValueError(f"Broken local link in {source.relative_to(ROOT)}: {target}")
        if resolved in names:
            url = names[resolved]
        else:
            kind = "tree" if resolved.is_dir() else "blob"
            url = f"{REPOSITORY}/{kind}/{quote(revision, safe='/')}/{resolved.relative_to(ROOT).as_posix()}"
        return f"]({url}{separator}{anchor})"

    return LINK.sub(link, text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true", help="validate without writing")
    mode.add_argument("--output", type=Path, help="export into a directory or wiki checkout")
    parser.add_argument("--source-ref", help="source Git revision used by repository links")
    args = parser.parse_args()
    revision = args.source_ref or subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
    ).strip()
    names = pages()
    rendered = {
        name: rewrite(path.read_text(encoding="utf-8"), path, names, revision)
        for path, name in names.items()
    }
    for path in [ROOT / "README.md", SOURCE / "README.md"]:
        rewrite(path.read_text(encoding="utf-8"), path, names, revision)
    if args.check:
        print(f"Validated {len(names)} wiki pages, language parity, and local links")
        return
    output = args.output.resolve()
    if output == ROOT or output == SOURCE or SOURCE.is_relative_to(output) or output.is_relative_to(SOURCE):
        raise ValueError("Choose an export directory outside the documentation source")
    output.mkdir(parents=True, exist_ok=True)
    source_url = f"{REPOSITORY}/tree/{quote(revision, safe='/')}/docs/wiki"
    for name, content in rendered.items():
        (output / f"{name}.md").write_text(
            f"{content.rstrip()}\n\n---\n[Documentation source / 文档来源]({source_url})\n",
            encoding="utf-8",
        )
    sidebar = ["[Languages / 语言](Home)", ""]
    for locale, title in [("en", "English"), ("zh-CN", "简体中文")]:
        sidebar.extend([f"**{title}**", ""])
        for path, name in names.items():
            if path.parent.name == locale:
                title = path.read_text(encoding="utf-8").splitlines()[0].removeprefix("# ")
                sidebar.append(f"- [{title}]({name})")
        sidebar.append("")
    (output / "_Sidebar.md").write_text("\n".join(sidebar), encoding="utf-8")
    print(f"Exported {len(names) + 1} pages to {output}; review the diff before publishing")


if __name__ == "__main__":
    try:
        main()
    except ValueError as error:
        raise SystemExit(str(error)) from error
