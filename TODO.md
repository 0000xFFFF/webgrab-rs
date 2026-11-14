- some images/videos don't have media file extension inside the url
  but the elements <img> will tell me it's an image or <video> if it's a video
  make sure --media also accepts those
- crawl should only crawl on pages it hasn't seen
add crawled pages to live csv so it should be like this:

type,url,filename,hash,is_dupe

wee need to add a column type it's either media or page
for pages the filename should be the title of the page
so rename filename column to just name
also hash the page why not

type,url,name,hash,is_dupe

