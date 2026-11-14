- add filters for crawl pages --any --must etc (--crawl-must, --crawl-must, ...)
- ditch the --color option add color by default (switch --color option with --nocolor)
- ditch the --count option add counting by default
- ditch the --valid option every url should be valid by default
- ditch the --grab option (most stuff is on by default we don't need it)
- replace --live-csv option with --db sqlite.db save relationships to sqlite database
  e.g. tables:
  WebPage — URL, title, HTTP status, timestamp.
  Media — URL, file path, MIME type, size, checksum, timestamp
  LINKS_TO — relationship between two pages.
  HAS_MEDIA — relationship between a page and media.
  REDIRECTS_TO — relationship between redirected pages.
  ... this is just an EXAMPLE, MAKE YOUR OWN TABLE/RELATIONSHIP NAMES THAT YOU THINK FIT BEST
  it would be nice if a tool existed to visualize the graph which page downloaded which files or something?
  i know neo4j has a cool graph but i don't want a neo4j server that manages the database that is overkill for me, i want a single file
  maybe tools exist that can visualize the sqlite db file if not use a different db to save the objects
  now we need to actually make WebPage, Media object when downloading and link them together...