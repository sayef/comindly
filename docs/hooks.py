"""Points links in files included from the repository root at their site pages or GitHub."""

import re
from pathlib import Path

from markdown import Extension
from markdown.preprocessors import Preprocessor

REPO = "https://github.com/sayef/comindly/blob/main/"
PAGES = {"CHANGELOG.md": "changelog.md", "CONTRIBUTING.md": "contributing.md", "ARCHITECTURE.md": "architecture.md"}
SNIPPET = re.compile(r'^--8<-- "([^"]+)"\s*$', re.M)
LINK = re.compile(
    r"\]\((CHANGELOG\.md|CONTRIBUTING\.md|ARCHITECTURE\.md|SECURITY\.md|CODE_OF_CONDUCT\.md|LICENSE)(#[^)]*)?\)"
)


class RepoLinks(Preprocessor):
    def run(self, lines):
        def target(match):
            name, anchor = match.group(1), match.group(2) or ""
            return f"]({PAGES.get(name, REPO + name)}{anchor})"

        return [LINK.sub(target, line) for line in lines]


class RepoLinksExtension(Extension):
    def extendMarkdown(self, md):
        md.preprocessors.register(RepoLinks(md), "comindly_repo_links", 20)


def on_config(config):
    config.markdown_extensions.append(RepoLinksExtension())
    return config


def on_page_markdown(markdown, page, config, **kwargs):
    """Keeps the page's Markdown, snippets inlined, for the Copy page button."""
    root = Path(config.config_file_path).parent
    page.cm_source = SNIPPET.sub(lambda m: (root / m.group(1)).read_text(), markdown).strip()
    return markdown
