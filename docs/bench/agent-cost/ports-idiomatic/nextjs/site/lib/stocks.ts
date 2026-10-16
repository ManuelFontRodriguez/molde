import data from "@/data/stocks.json";

export type Dividend = {
  ex_date: string;
  amount: string;
  currency: string;
  confirmed: boolean;
};

export type Stock = {
  ticker: string;
  name: string;
  sector: string;
  upcoming: Dividend | null;
};

type StockData = {
  cycle_id: string;
  stocks: Stock[];
};

const stockData: StockData = data;

export const cycleId = stockData.cycle_id;

export function getStocks(): Stock[] {
  return stockData.stocks;
}

export function getStock(ticker: string): Stock | undefined {
  return stockData.stocks.find((stock) => stock.ticker === ticker);
}

export function stockPath(ticker: string): string {
  return `/stocks/${encodeURIComponent(ticker)}/`;
}
