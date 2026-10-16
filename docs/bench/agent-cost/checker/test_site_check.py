#!/usr/bin/env python3
"""Tests for site-check: golden passes; each mutation fails with the right check id."""
import json, os, shutil, subprocess, sys, tempfile, unittest

HERE = os.path.dirname(os.path.abspath(__file__))
CHECK = os.path.join(HERE, "site-check")
GOLDEN = os.path.join(HERE, "..", "..", "cross-framework", "harness", "golden")
T = "stocks/T/index.html"


def run(d, *extra):
    p = subprocess.run([sys.executable, CHECK, d, *extra], capture_output=True, text=True)
    return p.returncode, (json.loads(p.stdout) if p.stdout else None)


class Cases(unittest.TestCase):
    def mutate(self, edit):
        d = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, d)
        out = os.path.join(d, "site")
        shutil.copytree(GOLDEN, out)
        edit(out)
        return run(out)

    def sub(self, rel, old, new):
        def f(out):
            p = os.path.join(out, rel)
            s = open(p).read()
            assert old in s, (rel, old)
            open(p, "w").write(s.replace(old, new, 1))
        return f

    def expect(self, edit, check, page=None):
        code, rep = self.mutate(edit)
        self.assertEqual(code, 1)
        self.assertEqual(rep["verdict"], "fail")
        found = [e for e in rep["errors"] if e["check"] == check]
        self.assertTrue(found, (check, rep["errors"]))
        self.assertTrue(all(e["hint"] for e in rep["errors"]))
        if page:
            self.assertIn(page, [e["page"] for e in found])

    def test_golden_passes(self):
        code, rep = run(GOLDEN)
        self.assertEqual((code, rep["verdict"], rep["errors"], rep["pages"]), (0, "pass", [], 5))
        self.assertEqual({n["requirement"] for n in rep["not_checked"]}, {1, 8})

    def test_usage(self):
        self.assertEqual(subprocess.run([sys.executable, CHECK], capture_output=True).returncode, 2)

    def test_broken_href(self):
        self.expect(self.sub(T, 'href="/"', 'href="/nope/"'), "links.internal", "/stocks/T/")

    def test_single_quoted_broken_href(self):
        self.expect(self.sub(T, 'href="/"', "href='/nope/'"), "links.internal")

    def test_unquoted_broken_href(self):
        self.expect(self.sub(T, 'href="/"', "href=/nope/"), "links.internal")

    def test_relative_broken_link(self):
        self.expect(self.sub(T, 'href="/"', 'href="../MISSING/"'), "links.internal")

    def test_relative_good_link_passes(self):
        code, _ = self.mutate(self.sub(T, 'href="/"', 'href="../KO/"'))
        self.assertEqual(code, 0)

    def test_srcset(self):
        self.expect(self.sub(T, "</body>", '<img src="/style.css" srcset="/a.png 1x, /style.css 2x"></body>'), "links.internal")

    def test_css_url_missing(self):
        def e(out):
            open(os.path.join(out, "style.css"), "a").write("\nbody{background:url('img/bg.png')}\n")
        self.expect(e, "links.internal", "style.css")

    def test_css_import_missing(self):
        def e(out):
            open(os.path.join(out, "style.css"), "a").write('\n@import "other.css";\n')
        self.expect(e, "links.internal", "style.css")

    def test_style_attribute(self):
        self.expect(self.sub(T, "<h1>", '<h1 style="background:url(/x.png)">'), "links.internal")

    def test_duplicate_title(self):
        def e(out):
            self.sub("stocks/KO/index.html", "<title>", "<title>AT&amp;T Inc (T)</title><x>")(out)
        # simpler: copy KO's title into T
        def e2(out):
            ko = open(os.path.join(out, "stocks/KO/index.html")).read()
            t = ko.split("<title>")[1].split("</title>")[0]
            self.sub(T, "AT&amp;T Inc (T)</title>", t + "</title>")(out)
        self.expect(e2, "titles.duplicate", "/stocks/T/")

    def test_missing_title(self):
        self.expect(self.sub(T, "<title>AT&amp;T Inc (T)</title>", ""), "titles.missing")

    def test_missing_from_sitemap(self):
        self.expect(self.sub("sitemap.xml", "  <url><loc>https://demo.example/stocks/T/</loc></url>\n", ""),
                    "sitemap.missing_page", "/stocks/T/")

    def test_extra_in_sitemap(self):
        self.expect(self.sub("sitemap.xml", "</urlset>", "<url><loc>https://demo.example/ghost/</loc></url></urlset>"),
                    "sitemap.dangling")

    def test_wrong_canonical(self):
        self.expect(self.sub(T, 'rel="canonical" href="https://demo.example/stocks/T/"',
                             'rel="canonical" href="https://demo.example/stocks/T"'), "canonical.mismatch")

    def test_og_url_mismatch(self):
        self.expect(self.sub(T, 'og:url" content="https://demo.example/stocks/T/"',
                             'og:url" content="https://demo.example/T/"'), "og.url_mismatch")

    def test_og_title_mismatch(self):
        self.expect(self.sub(T, 'og:title" content="AT&amp;T Inc (T)"', 'og:title" content="Other"'), "og.title_mismatch")

    def test_double_escaped(self):
        self.expect(self.sub(T, "<h1>AT&amp;T", "<h1>AT&amp;amp;T"), "escaping.double", "/stocks/T/")

    def test_invalid_jsonld(self):
        self.expect(self.sub(T, "</head>", '<script type="application/ld+json">{"a": 1,}</script></head>'), "jsonld.invalid")

    def test_valid_jsonld_passes(self):
        code, _ = self.mutate(self.sub(T, "</head>", '<script type="application/ld+json">{"a": "AT&amp;T"}</script></head>'))
        self.assertEqual(code, 0)

    def test_missing_robots(self):
        self.expect(lambda out: os.remove(os.path.join(out, "robots.txt")), "robots.missing")

    def test_robots_without_sitemap(self):
        self.expect(lambda out: open(os.path.join(out, "robots.txt"), "w").write("User-agent: *\n"), "robots.sitemap")

    def test_framework_and_404_ignored(self):
        def e(out):
            os.makedirs(os.path.join(out, "_assets"))
            open(os.path.join(out, "_assets", "x.html"), "w").write("<p>no title</p>")
            open(os.path.join(out, "404.html"), "w").write("<p>no title</p>")
        code, rep = self.mutate(e)
        self.assertEqual(code, 0, rep)


if __name__ == "__main__":
    unittest.main(verbosity=1)
