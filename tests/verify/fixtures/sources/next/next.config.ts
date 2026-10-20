import type { NextConfig } from "next";

// create-next-app 16.4 enables cacheComponents and partialPrefetching. Both imply
// partial prerendering, which static export rejects ("PPR cannot be enabled in export
// mode"), so a static export drops them.
const nextConfig: NextConfig = {
  output: "export",
  trailingSlash: true,
};

export default nextConfig;
