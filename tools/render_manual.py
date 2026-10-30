#!/usr/bin/env python3
"""Render docs/guides/GAME_MANUAL.md into the site's manual page and copy the images it uses.

Usage: tools/render_manual.py <manual.md> <template.html> <out_dir>

Handles exactly the Markdown the manual uses: ATX headings, paragraphs, a blockquote, `-` lists
nested by four spaces, pipe tables, `term` / `: definition` lists, and inline code, images, links,
bold and backslash escapes. Heading ids follow GitHub's, so links into the GitHub copy match. A link
out of docs/ points at the file on GitHub, and an HTML comment line is left out.
"""
import html
import re
import shutil
import struct
import sys
from pathlib import Path

HEADING = re.compile(r'^(#{1,6}) +(.*?) *#*$')
ITEM = re.compile(r'^( *)- +(.*)$')
TABLE_RULE = re.compile(r'^\|? *:?-+:? *(\| *:?-+:? *)*\|? *$')
IMAGE = re.compile(r'!\[([^\]]*)\]\(([^)\s]+)\)')
LINK = re.compile(r'\[([^\]]+)\]\(([^)\s]+)\)')
REPO = 'https://github.com/KyleJamesWalker/last-harvest/blob/main/'


def href(target):
    return REPO + target[len('../../'):] if target.startswith('../../') else target


class Manual:
    def __init__(self, src_dir):
        self.src_dir = src_dir
        self.images = []
        self.headings = []
        self.slugs = {}

    def image(self, alt, src, inline):
        path = self.src_dir / src
        with open(path, 'rb') as f:
            width, height = struct.unpack('>II', f.read(24)[16:24])
        if src not in self.images:
            self.images.append(src)
        extra = 'class="icon"' if inline else 'loading="lazy"'
        return f'<img src="{html.escape(src)}" alt="{html.escape(alt)}" width="{2 * width}" height="{2 * height}" {extra}>'

    def inline(self, text, in_heading=False):
        held = []

        def hold(fragment):
            held.append(fragment)
            return f'\0{len(held) - 1}\0'

        text = re.sub(r'`([^`]+)`', lambda m: hold(f'<code>{html.escape(m[1])}</code>'), text)
        text = re.sub(r'\\([\\`*_\[\]()#|!-])', lambda m: hold(html.escape(m[1])), text)
        text = html.escape(text, quote=False)
        text = IMAGE.sub(lambda m: hold(self.image(html.unescape(m[1]), html.unescape(m[2]), in_heading)), text)
        text = LINK.sub(lambda m: hold(f'<a href="{html.escape(href(html.unescape(m[2])))}">{m[1]}</a>'), text)
        text = re.sub(r'\*\*(.+?)\*\*', r'<strong>\1</strong>', text)
        while '\0' in text:
            text = re.sub(r'\0(\d+)\0', lambda m: held[int(m[1])], text)
        return text

    def slug(self, text):
        plain = re.sub(r'<[^>]+>', '', text)
        base = re.sub(r'[^\w\- ]', '', html.unescape(plain).lower()).replace(' ', '-')
        n = self.slugs.get(base, 0)
        self.slugs[base] = n + 1
        return base if n == 0 else f'{base}-{n}'

    def blocks(self, lines):
        out = []
        i = 0
        while i < len(lines):
            line = lines[i]
            if not line.strip() or line.startswith('<!--'):
                i += 1
                continue
            if m := HEADING.match(line):
                level, content = len(m[1]), self.inline(m[2], in_heading=True)
                hid = self.slug(content)
                self.headings.append((level, hid, content))
                out.append(f'<h{level} id="{hid}">{content}<a class="anchor" href="#{hid}" aria-label="Link to this section">#</a></h{level}>')
                i += 1
            elif line.startswith('>'):
                quoted = []
                while i < len(lines) and lines[i].startswith('>'):
                    quoted.append(lines[i][1:].removeprefix(' '))
                    i += 1
                out.append(f'<blockquote>\n{self.blocks(quoted)}\n</blockquote>')
            elif line.startswith('|') and i + 1 < len(lines) and TABLE_RULE.match(lines[i + 1]):
                i = self.table(lines, i, out)
            elif ITEM.match(line):
                i = self.list(lines, i, out)
            elif i + 1 < len(lines) and lines[i + 1].startswith(': '):
                i = self.definitions(lines, i, out)
            else:
                para = [line.strip()]
                i += 1
                while i < len(lines) and lines[i].strip() and not self.starts_block(lines, i):
                    para.append(lines[i].strip())
                    i += 1
                text = ' '.join(para)
                cls = ' class="shot"' if IMAGE.fullmatch(text) else ''
                out.append(f'<p{cls}>{self.inline(text)}</p>')
        return '\n'.join(out)

    @staticmethod
    def starts_block(lines, i):
        line = lines[i]
        return bool(HEADING.match(line) or ITEM.match(line) or line.startswith(('>', '|', '<!--'))
                    or (i + 1 < len(lines) and lines[i + 1].startswith(': ')))

    @staticmethod
    def cells(line):
        return [c.strip() for c in re.split(r'(?<!\\)\|', line.strip().strip('|'))]

    def table(self, lines, i, out):
        align = []
        for rule in self.cells(lines[i + 1]):
            align.append('center' if rule.startswith(':') and rule.endswith(':') else
                         'right' if rule.endswith(':') else 'left' if rule.startswith(':') else '')

        def row(tag, line):
            cells = []
            for n, cell in enumerate(self.cells(line)):
                style = f' style="text-align:{align[n]}"' if n < len(align) and align[n] else ''
                cells.append(f'<{tag}{style}>{self.inline(cell)}</{tag}>')
            return '<tr>' + ''.join(cells) + '</tr>'

        body = []
        j = i + 2
        while j < len(lines) and lines[j].startswith('|'):
            body.append(row('td', lines[j]))
            j += 1
        out.append(f'<div class="table"><table>\n<thead>{row("th", lines[i])}</thead>\n<tbody>\n'
                   + '\n'.join(body) + '\n</tbody>\n</table></div>')
        return j

    def list(self, lines, i, out):
        # (indent, text) per item; a non-item line continues the item above it.
        items = []
        while i < len(lines):
            if m := ITEM.match(lines[i]):
                items.append([len(m[1]), m[2].strip()])
            elif lines[i].strip() and not self.starts_block(lines, i) and items:
                items[-1][1] += ' ' + lines[i].strip()
            elif not lines[i].strip() and i + 1 < len(lines) and ITEM.match(lines[i + 1]):
                pass
            else:
                break
            i += 1
        out.append(self.nest(items, 0)[0])
        return i

    def nest(self, items, k):
        indent = items[k][0]
        parts = ['<ul>']
        while k < len(items) and items[k][0] >= indent:
            item = f'<li>{self.inline(items[k][1])}'
            k += 1
            if k < len(items) and items[k][0] > indent:
                sub, k = self.nest(items, k)
                item += '\n' + sub + '\n'
            parts.append(item + '</li>')
        parts.append('</ul>')
        return '\n'.join(parts), k

    def definitions(self, lines, i, out):
        parts = ['<dl>']
        while i + 1 < len(lines) and lines[i + 1].startswith(': '):
            parts.append(f'<dt>{self.inline(lines[i].strip())}</dt>')
            body = [lines[i + 1][2:]]
            i += 2
            while i < len(lines) and (not lines[i].strip() or lines[i].startswith('    ')):
                body.append(lines[i][4:])
                i += 1
            parts.append(f'<dd>\n{self.blocks(body)}\n</dd>')
            while i < len(lines) and not lines[i].strip():
                i += 1
        parts.append('</dl>')
        out.append('\n'.join(parts))
        return i

    def toc(self):
        parts = ['<ol>']
        open_sub = False
        for level, hid, content in self.headings:
            if level == 2:
                if open_sub:
                    parts.append('</ol>')
                    open_sub = False
                if len(parts) > 1:
                    parts.append('</li>')
                parts.append(f'<li><a href="#{hid}">{content}</a>')
            elif level == 3 and len(parts) > 1:
                if not open_sub:
                    parts.append('<ol>')
                    open_sub = True
                parts.append(f'<li><a href="#{hid}">{content}</a></li>')
        if open_sub:
            parts.append('</ol>')
        parts.append('</li></ol>')
        return '\n'.join(parts)


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    md, template, out_dir = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    manual = Manual(md.parent)
    content = manual.blocks(md.read_text(encoding='utf-8').splitlines())
    page = template.read_text(encoding='utf-8')
    page = page.replace('{{ TOC }}', manual.toc()).replace('{{ CONTENT }}', content)
    (out_dir / 'manual.html').write_text(page, encoding='utf-8')
    for src in manual.images:
        (out_dir / src).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(md.parent / src, out_dir / src)


if __name__ == '__main__':
    main()
