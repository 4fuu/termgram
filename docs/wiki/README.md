# Maintaining the Wiki

[English guide](en/Home.md) · [简体中文指南](zh-CN/Home.md)

This directory is the reviewed source for the project Wiki. User documentation
is tracked with its features; local development plans stay in Git-ignored
`dev-notes/`. Keep the same page filenames in `en/` and `zh-CN/`, use relative
inline Markdown links, and update both languages in the feature's PR.

The exporter uses Python 3.9+ standard-library modules. It validates language
parity and local link targets, flattens `en/Configuration.md` to
`en-Configuration.md`, rewrites links, and generates a language sidebar. It does
not render Markdown, push Git commits, or remove unrelated Wiki pages.

```sh
python3 scripts/export-wiki.py --check
python3 scripts/export-wiki.py --output dist/wiki
```

Repository links in exports point to the current commit. Export from a clean,
published source revision; `--source-ref REF` can select another published ref.
Documentation on an unmerged branch describes that branch, not necessarily the
latest installer release. Source pages remain browsable on GitHub without export.

To publish the approved source, initialize the Wiki with one page on GitHub if it
does not already exist, then clone its separate repository:

```sh
git clone https://github.com/iebb/termgram.wiki.git ../termgram.wiki
python3 scripts/export-wiki.py --output ../termgram.wiki
git -C ../termgram.wiki diff
git -C ../termgram.wiki add -- '*.md'
git -C ../termgram.wiki commit -m "docs: synchronize bilingual user guides"
git -C ../termgram.wiki push
```

Review renamed/deleted pages manually as part of the diff. GitHub only publishes
the Wiki's default branch. See the official
[Wiki editing instructions](https://docs.github.com/en/communities/documenting-your-project-with-wikis/adding-or-editing-wiki-pages).
The normal feature PR does not automatically merge or publish Wiki changes.

这里是 Wiki 的可审阅源文件。英文与简体中文保持相同页名，使用相对 Markdown 链接，
随功能 PR 同步修改。`--check` 校验两种语言的页面和内部链接；`--output` 导出为
GitHub Wiki 的扁平页名并生成语言侧栏。脚本不联网、不推送、不删除无关页面。
从干净且已发布的源码提交导出，审阅 Wiki diff 后再提交推送；首次发布需要在 GitHub
先创建一个页面。未合并分支的文档可能包含最新发行版尚未提供的功能。
