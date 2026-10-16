import type { Metadata } from "next";
import Link from "next/link";
import { cycleId, getStocks, stockPath } from "@/lib/stocks";

const title = "Demo — dividend stocks";

export const metadata: Metadata = {
  title,
  alternates: { canonical: "/" },
  openGraph: { title, url: "/", type: "website" },
};

export default function HomePage() {
  return (
    <main>
      <h1>Stocks (cycle {cycleId})</h1>
      <ul>
        {getStocks().map((stock) => (
          <li key={stock.ticker}>
            <Link href={stockPath(stock.ticker)}>{stock.name}</Link>
            {" — "}
            {stock.sector}
          </li>
        ))}
      </ul>
    </main>
  );
}
