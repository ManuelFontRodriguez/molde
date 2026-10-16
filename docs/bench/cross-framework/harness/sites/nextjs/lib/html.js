// Client-safe helpers (no node builtins).
export const BASE_URL = 'https://demo.example';

// Minimal HTML escaping (AT&T -> AT&amp;T), matching the golden output.
export function esc(s) {
  return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}
