# A site broken on purpose

This is the [demo](../demo) with five mistakes of the kind that slip through review,
and the report `molde check` gives for it, committed as
[expected-report.json](expected-report.json) (without the `build` object, whose id and
timing change on every run). Run it yourself:

```sh
molde check --site examples/broken --out /tmp/broken-out
```

The verdict is `fail`, so nothing is published. Every error names the page, and the
route and template that made it.

| What is wrong | Where | What molde reports |
|---|---|---|
| A link with a typo, `/books/dnue/` | `templates/index.html.j2` | `links.internal` on `index.html` |
| A font the stylesheet loads but the site doesn't have | `static/style.css` | `links.internal` on `style.css`, attribute `url()` |
| One title for every genre page, `Genre — Demo library` | `routes.yaml`, the genre route | `titles.duplicate` on two of the three genre pages, each naming the first |
| The route promises `Book` structured data; the template writes `CreativeWork` | `templates/book.html.j2` | `jsonld.type_missing` on every book page, with what was `found` |
| A title that is already HTML-escaped in the data, `Pride &amp; Prejudice` | `data.json` | `html.double_escaped` on every page that prints it: the browser would show `&amp;` |

None of these stops a page from rendering, and a browser shows most of them only to a
visitor who clicks the link, opens the right page, or reads the page source.

The test suite runs this site and compares the report with `expected-report.json`, so
the file is also the reference for what each check's error looks like.
