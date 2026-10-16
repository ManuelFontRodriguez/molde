"""The port factory's per-framework parameters (DESIGN.md §4). A factory run is a T0 run
in the agent container (no molde context: the arm note names only the framework, and the
prompt is tasks/factory.md filled in from here). Its output must pass the whole judge;
the first passing run per framework is frozen into ports-idiomatic/<fw>/site/ and
published with its transcript, for experts to improve by pull request.
"""

FACTORY = {
    "hugo": dict(framework="Hugo", port="idiomatic-hugo",
                 builtins="Hugo's built-in sitemap and robots.txt templates, `.Permalink`, data files, "
                          "content adapters or other built-in ways to make pages from data, base templates and partials",
                 build="hugo --destination public --cleanDestinationDir", out="public", data_path="data/stocks.json"),
    "astro": dict(framework="Astro", port="idiomatic-astro",
                  builtins="components and layouts, `getStaticPaths`, `Astro.site`, the installed `@astrojs/sitemap` "
                           "integration (or Astro's own endpoints where the integration's output wouldn't meet the requirements)",
                  build="npx astro build", out="dist", data_path="src/data/stocks.json"),
    "nextjs": dict(framework="Next.js", port="idiomatic-nextjs",
                   builtins="the App Router, the Metadata API (`metadata` / `generateMetadata`, `metadataBase`), "
                            "`app/sitemap.ts` and `app/robots.ts`, `generateStaticParams`, a static export, TypeScript",
                   build="npm run build", out="out", data_path="data/stocks.json"),
}


def prompt(fw, template):
    return template.format(**FACTORY[fw])
