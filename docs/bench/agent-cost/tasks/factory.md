Build the website described below in this folder with {framework}, written the way an experienced {framework} developer would write a small production site today: idiomatic project structure, and the framework's own built-in features wherever they exist ({builtins}). Don't imitate any other tool's output format; correct, normal {framework} code is the goal.

Constraints, because other tools will build and edit this project later:
- Keep the data in `{data_path}` (move `data.json` there); pages must be generated from it.
- `{build}` must build the site into `{out}/`, with the site's base URL `https://demo.example`.
- Use only the dependencies already installed (there is no internet access).

The site:

- The home page `/` has the title "Demo — dividend stocks", a heading "Stocks (cycle <cycle_id>)" and a list of every stock in the data, in order: its name as a link to its page, then " — " and its sector.
- Each stock has a page at `/stocks/<TICKER>/` (the ticker exactly as written in the data) with the title "<name> (<TICKER>)". It shows the name with the ticker, the sector, and either the upcoming dividend as "Next dividend: <amount> <currency> — ex <ex_date>" or, when `upcoming` is null, "No upcoming dividend declared." When the upcoming dividend is not confirmed (`confirmed: false`) the page also shows a badge, an element with class `badge`, reading "Estimated — not yet declared". Each stock page links back to `/`.
- Every page loads `/style.css` (the file `style.css` here, unchanged).
- `/sitemap.xml` and `/robots.txt` as the requirements say; every page also has `og:type` "website".
- `<html lang="en">` on every page.
