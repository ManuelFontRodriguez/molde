module.exports = function (eleventyConfig) {
  eleventyConfig.addPassthroughCopy({ "src/style.css": "style.css" });

  eleventyConfig.addTemplate(
    "robots.njk",
    "User-agent: *\nAllow: /\n\nSitemap: https://demo.example/sitemap.xml\n",
    { permalink: "/robots.txt" }
  );

  eleventyConfig.addTemplate(
    "sitemap.njk",
    '<?xml version="1.0" encoding="UTF-8"?>\n' +
      '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
      "  <url><loc>https://demo.example/</loc></url>\n" +
      "{% for s in demo.stocks %}  <url><loc>https://demo.example/stocks/{{ s.ticker }}/</loc></url>\n" +
      "{% endfor %}</urlset>\n",
    { permalink: "/sitemap.xml" }
  );

  return { dir: { input: "src", output: "_site" } };
};
