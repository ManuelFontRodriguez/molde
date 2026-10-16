import type { MetadataRoute } from "next";
import { siteUrl } from "@/lib/site";
import { getStocks, stockPath } from "@/lib/stocks";

export const dynamic = "force-static";

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { url: `${siteUrl}/` },
    ...getStocks().map((stock) => ({ url: `${siteUrl}${stockPath(stock.ticker)}` })),
  ];
}
