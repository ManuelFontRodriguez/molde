// Build-time only (uses fs): import solely from getStaticProps/getStaticPaths,
// which Next strips from the client bundle.
const fs = require('fs');
const path = require('path');

// Shared demo-site inputs (data + static assets). Override with DEMO_SITE_DIR.
const DEMO_SITE_DIR = process.env.DEMO_SITE_DIR || '/tmp/molde-vs/shared/demo-site';

function getData() {
  return JSON.parse(fs.readFileSync(path.join(DEMO_SITE_DIR, 'data.json'), 'utf8'));
}

module.exports = { DEMO_SITE_DIR, getData };
