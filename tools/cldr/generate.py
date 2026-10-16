#!/usr/bin/env python3
"""Generate src/i18n/cldr_data.rs from pinned cldr-json packages.

    python3 tools/cldr/generate.py            # download, verify, write
    python3 tools/cldr/generate.py --cache /tmp/cldr   # keep the tarballs

Standard library only. The tarballs are the npm releases of unicode-org/cldr-json,
pinned by version AND sha512 integrity, so the same script always writes the same
bytes. Bumping CLDR is a deliberate change: edit VERSION + INTEGRITY, regenerate, and
review the golden-test diff (tools/cldr/README.md).

What is extracted, per locale (Gregorian calendar only, see the design doc):
months and weekdays (format + stand-alone, wide/abbreviated/narrow), the CE era name,
the full/long/medium/short date patterns plus the yMMMM skeleton, and the default
numbering system's digits, decimal/group/minus symbols and decimal grouping sizes.
Locales: CLDR effective coverage basic or better (what ICU4X ships by default).
"""

import argparse
import base64
import hashlib
import io
import json
import os
import re
import sys
import tarfile
import unicodedata
import urllib.request

VERSION = "48.0.0"
PACKAGES = {
    "cldr-core": "sha512-iQnK68eOjEsHEOwCvi2CI7xbjcVlCdaSNTQrkwCgEMnyZUbHCgF0GPw2CXi0D2IGz+9Pj80cmcWzRFcF6zdrYQ==",
    "cldr-dates-full": "sha512-RcJk/KQzmL9UlE5rAtS4Vxbi0lPGTXZ+s6ksqKY14HLQh3L3kewYlwK146l46uRS9JOhSlXhGV+V3KrUmKMioQ==",
    "cldr-numbers-full": "sha512-bPTCfVDc0KRdRCRES155EZX1U0ZjUnc9AgiyKPH7K+mXWgCPLx7CzxvYY0IXGTF8W6ig+2hbG5V7oHMenA8iGQ==",
}
COVERAGE = {"basic", "moderate", "modern"}
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.path.join(ROOT, "src", "i18n", "cldr_data.rs")
LICENSE_OUT = os.path.join(ROOT, "src", "i18n", "LICENSE-UNICODE")
# Pattern letters the Rust interpreter implements (src/i18n.rs `format_pattern`).
SUPPORTED_FIELDS = re.compile(r"^(G{1,4}|y+|M{1,5}|L{1,5}|d{1,2}|E{1,5}|c{3,5})$")


def fetch(name, cache):
    url = f"https://registry.npmjs.org/{name}/-/{name}-{VERSION}.tgz"
    path = os.path.join(cache, f"{name}-{VERSION}.tgz") if cache else None
    if path and os.path.exists(path):
        data = open(path, "rb").read()
    else:
        with urllib.request.urlopen(url) as r:
            data = r.read()
        if path:
            os.makedirs(cache, exist_ok=True)
            open(path, "wb").write(data)
    want = PACKAGES[name]
    got = "sha512-" + base64.b64encode(hashlib.sha512(data).digest()).decode()
    if got != want:
        sys.exit(f"{name}-{VERSION}: integrity mismatch\n  want {want}\n  got  {got}")
    return tarfile.open(fileobj=io.BytesIO(data), mode="r:gz")


class Package:
    def __init__(self, tar):
        self.tar = tar
        self.names = set(tar.getnames())

    def json(self, rel):
        return json.load(self.tar.extractfile("package/" + rel))

    def has(self, rel):
        return "package/" + rel in self.names

    def raw(self, rel):
        return self.tar.extractfile("package/" + rel).read()


def check_pattern(tag, pat):
    for kind, text in tokens(pat):
        if kind == "field" and not SUPPORTED_FIELDS.match(text):
            sys.exit(f"{tag}: date pattern {pat!r} uses unsupported field {text!r}")


def tokens(pat):
    """LDML pattern -> [(field|literal, text)]; mirrors the Rust tokenizer."""
    out, i = [], 0
    while i < len(pat):
        c = pat[i]
        if c == "'":
            if pat[i + 1 : i + 2] == "'":
                out.append(("literal", "'"))
                i += 2
                continue
            j = pat.find("'", i + 1)
            while j != -1 and pat[j + 1 : j + 2] == "'":
                j = pat.find("'", j + 2)
            if j == -1:
                sys.exit(f"unterminated quote in {pat!r}")
            out.append(("literal", pat[i + 1 : j].replace("''", "'")))
            i = j + 1
        elif c.isascii() and c.isalpha():
            j = i
            while j < len(pat) and pat[j] == c:
                j += 1
            out.append(("field", pat[i:j]))
            i = j
        else:
            out.append(("literal", c))
            i += 1
    return out


def grouping(tag, pat):
    """'#,##,##0.###' -> (3, 2). Only plain decimal patterns are expected."""
    if not re.fullmatch(r"[#0,]+(\.[#0]+)?", pat):
        sys.exit(f"{tag}: unsupported decimal pattern {pat!r}")
    integer = pat.split(".")[0]
    groups = integer.split(",")
    if len(groups) < 2:
        sys.exit(f"{tag}: decimal pattern {pat!r} has no grouping")
    primary = len(groups[-1])
    secondary = len(groups[-2]) if len(groups) > 2 else primary
    return primary, secondary


def rust_str(s):
    out = []
    for ch in s:
        cat = unicodedata.category(ch)
        if ch == "\\":
            out.append("\\\\")
        elif ch == '"':
            out.append('\\"')
        elif ch != " " and (cat[0] in "CZ" or cat == "Mn" and not out):
            out.append("\\u{%04X}" % ord(ch))
        else:
            out.append(ch)
    return '"' + "".join(out) + '"'


def rust_char(ch):
    return "'" + rust_str(ch)[1:-1].replace("\\\"", '"') + "'"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", help="directory to keep the downloaded tarballs")
    args = ap.parse_args()

    core = Package(fetch("cldr-core", args.cache))
    dates = Package(fetch("cldr-dates-full", args.cache))
    numbers = Package(fetch("cldr-numbers-full", args.cache))

    coverage = core.json("coverageLevels.json")["effectiveCoverageLevels"]
    numbering = core.json("supplemental/numberingSystems.json")["supplemental"]["numberingSystems"]
    likely = core.json("supplemental/likelySubtags.json")["supplemental"]["likelySubtags"]
    lang_alias = core.json("supplemental/aliases.json")["supplemental"]["metadata"]["alias"][
        "languageAlias"
    ]

    tags = sorted(
        t
        for t, level in coverage.items()
        if level in COVERAGE
        and t != "und"  # root: "M01"-style names, not a language
        and dates.has(f"main/{t}/ca-gregorian.json")
        and numbers.has(f"main/{t}/numbers.json")
    )

    arrays = {}  # tuple -> name
    array_defs = []

    def intern(prefix, values):
        key = (prefix, tuple(values))
        if key not in arrays:
            name = f"{prefix}{len([k for k in arrays if k[0] == prefix])}"
            arrays[key] = name
            array_defs.append((name, len(values), values))
        return arrays[key]

    digit_sets = {}
    locales = []
    for tag in tags:
        g = dates.json(f"main/{tag}/ca-gregorian.json")["main"][tag]["dates"][
            "calendars"
        ]["gregorian"]
        months = []
        days = []
        for ctx in ("format", "stand-alone"):
            for width in ("wide", "abbreviated", "narrow"):
                m = g["months"][ctx][width]
                months.append(intern("M", [m[str(i)] for i in range(1, 13)]))
                d = g["days"][ctx][width]
                days.append(
                    intern("D", [d[k] for k in ("sun", "mon", "tue", "wed", "thu", "fri", "sat")])
                )
        era = [g["eras"]["eraAbbr"]["1"], g["eras"]["eraNames"]["1"]]
        pats = []
        for style in ("full", "long", "medium", "short"):
            p = g["dateFormats"][style]
            if isinstance(p, dict):
                sys.exit(f"{tag}: {style} date pattern has a numbering override {p!r}")
            pats.append(p)
        pats.append(g["dateTimeFormats"]["availableFormats"]["yMMMM"])
        for p in pats:
            check_pattern(tag, p)

        n = numbers.json(f"main/{tag}/numbers.json")["main"][tag]["numbers"]
        ns = n["defaultNumberingSystem"]
        system = numbering[ns]
        if system["_type"] != "numeric":
            sys.exit(f"{tag}: default numbering system {ns} is not numeric")
        digits = system["_digits"]
        if len(digits) != 10:
            sys.exit(f"{tag}: numbering system {ns} does not have 10 digits")
        if digits not in digit_sets:
            digit_sets[digits] = (f"DIGITS_{ns.upper()}", ns)
        sym = n[f"symbols-numberSystem-{ns}"]
        prim, sec = grouping(tag, n[f"decimalFormats-numberSystem-{ns}"]["standard"])
        locales.append(
            (tag, months, days, era, pats, digit_sets[digits][0], sym["decimal"], sym["group"], sym["minusSign"], prim, sec)
        )

    # lang-REGION tags that change script (zh-TW -> zh-Hant): the only regional tags
    # whose data is not found by dropping the region.
    have = set(tags)

    def script_of(t):
        return likely.get(t, "").split("-")[1] if likely.get(t) else None

    aliases = {}
    for key, full in likely.items():
        m = re.fullmatch(r"([a-z]{2,3})-([A-Z]{2})", key)
        if not m or key in have:
            continue
        lang, region = m.groups()
        script = full.split("-")[1]
        if lang not in have or script == script_of(lang):
            continue
        for cand in (f"{lang}-{script}-{region}", f"{lang}-{script}"):
            if cand in have:
                aliases[key] = cand
                break
    for old, v in lang_alias.items():
        new = v["_replacement"]
        if re.fullmatch(r"[a-z]{2}", old) and old not in have and new in have:
            aliases[old] = new

    lines = []
    w = lines.append
    w("// @generated by tools/cldr/generate.py — do not edit by hand; regenerate instead.")
    w(f"// Source: cldr-json {VERSION} (cldr-core, cldr-dates-full, cldr-numbers-full), npm tarballs")
    w("// pinned by sha512. Unicode CLDR data, Unicode License v3: see LICENSE-UNICODE here.")
    w("")
    w("use super::CldrLocale;")
    w("")
    w(f'pub const CLDR_VERSION: &str = "{VERSION}";')
    w("")
    for digits, (name, ns) in sorted(digit_sets.items(), key=lambda kv: kv[1][0]):
        w(f"static {name}: [char; 10] = [{', '.join(rust_char(c) for c in digits)}];")
    w("")
    for name, size, values in array_defs:
        w(f"static {name}: [&str; {size}] = [{', '.join(rust_str(v) for v in values)}];")
    w("")
    w("/// Sorted by tag (binary search).")
    w(f"pub static LOCALES: [CldrLocale; {len(locales)}] = [")
    for tag, months, days, era, pats, digits, dec, grp, minus, prim, sec in locales:
        w("    CldrLocale {")
        w(f'        tag: "{tag}",')
        w(f"        months: [{', '.join('&' + m for m in months)}],")
        w(f"        days: [{', '.join('&' + d for d in days)}],")
        w(f"        era: [{', '.join(rust_str(e) for e in era)}],")
        w(f"        dates: [{', '.join(rust_str(p) for p in pats)}],")
        w(f"        digits: &{digits},")
        w(f"        decimal: {rust_str(dec)},")
        w(f"        group: {rust_str(grp)},")
        w(f"        minus: {rust_str(minus)},")
        w(f"        grouping: [{prim}, {sec}],")
        w("    },")
    w("];")
    w("")
    w("/// Tags whose data lives under another tag: a script change (`zh-TW` -> `zh-Hant`)")
    w("/// or a deprecated language code (`iw` -> `he`). Sorted.")
    w(f"pub static ALIASES: [(&str, &str); {len(aliases)}] = [")
    for k in sorted(aliases):
        w(f'    ("{k}", "{aliases[k]}"),')
    w("];")
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    with open(LICENSE_OUT, "wb") as f:
        f.write(core.raw("LICENSE"))
    print(f"wrote {os.path.relpath(OUT, ROOT)}: {len(locales)} locales, {len(array_defs)} arrays, {len(aliases)} aliases, {os.path.getsize(OUT)} bytes")


if __name__ == "__main__":
    main()
