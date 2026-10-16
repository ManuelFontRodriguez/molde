1. Add a new field to each item (e.g. `price`) in the JSON and display it on both the item page and its index entry.
2. Change the index sort order (e.g. newest first by `date_added`, or alphabetical by `name`) instead of JSON file order.
3. Add category/tag filtering to the index: a row of tag buttons that shows only items whose `tags` array contains the selected tag.
4. Paginate the index at a fixed page size (e.g. 20 items per page) with previous/next links and generated `/page/N/` URLs.
5. Add a client-side search box on the index that filters listed items by matching text in `name` and `description`.
