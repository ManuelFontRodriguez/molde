import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { getStock, getStocks, stockPath } from "@/lib/stocks";

type Props = {
  params: Promise<{ ticker: string }>;
};

export const dynamicParams = false;

export function generateStaticParams() {
  return getStocks().map((stock) => ({ ticker: stock.ticker }));
}

async function loadStock(params: Props["params"]) {
  const { ticker } = await params;
  const stock = getStock(ticker);
  if (!stock) notFound();
  return stock;
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const stock = await loadStock(params);
  const title = `${stock.name} (${stock.ticker})`;
  const url = stockPath(stock.ticker);

  return {
    title,
    alternates: { canonical: url },
    openGraph: { title, url, type: "website" },
  };
}

export default async function StockPage({ params }: Props) {
  const stock = await loadStock(params);
  const { upcoming } = stock;

  return (
    <main>
      <h1>
        {stock.name} <small>({stock.ticker})</small>
      </h1>
      <p>Sector: {stock.sector}</p>
      {upcoming ? (
        <p className={upcoming.confirmed ? "upcoming" : "upcoming estimated"}>
          Next dividend: {upcoming.amount} {upcoming.currency} — ex{" "}
          {upcoming.ex_date}
          {!upcoming.confirmed && (
            <>
              {" "}
              <span className="badge">Estimated — not yet declared</span>
            </>
          )}
        </p>
      ) : (
        <p>No upcoming dividend declared.</p>
      )}
      <p>
        <Link href="/">Back to all stocks</Link>
      </p>
    </main>
  );
}
