use rss::Channel;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::result::Result;
use url::Url;

use crate::common::*;

/// Open the file and use the rss crate to read the file and get a list of Items (namely, feeds).
/// For each feed in the list, get the URL and call `process_feed`. Take a look at the examples in
/// the rss crate.
pub fn process_feed_file(file_name: &str, index: &mut ArticleIndex) -> RssIndexResult<()> {
    let file = File::open(file_name)?;
    println!("Processing feed file: {}", file_name);

    let channel = Channel::read_from(BufReader::new(file))?;

    let mut urls = HashSet::new();

    for feed in channel.into_items() {
        let url = feed.link().ok_or(RssIndexError::UrlError)?;
        let title = feed.title().ok_or(RssIndexError::UrlError)?;

        if urls.contains(url) {
            println!("Skipping already seen feed: {} [{}]", title, url);
            continue;
        }
        urls.insert(url.to_string());

        println!("Processing feed: {} [{}]", title, url);
        process_feed(url, index, &mut urls)?;
    }
    Result::Ok(())
}

/// Read a feed file from a URL, build an rss channel from it, and iterate through the `Item`s
/// (articles). Pull out three pieces of information: the URL, the hostname, and the title (see
/// Item::link, Url::parse, and Url::host_str here). Process each url/title with `process_article`,
/// and then add it to the input ArticleIndex along with the hostname. If an Item is missing a
/// url/hostname/title, skip it (do not panic).
fn process_feed(
    url: &str,
    index: &mut ArticleIndex,
    urls: &mut HashSet<String>,
) -> RssIndexResult<()> {
    let contents = reqwest::blocking::get(url)?.bytes()?;
    let channel = Channel::read_from(&contents[..])?;
    let items = channel.into_items();
    for item in items {
        let (url, site, title) = match (item.link(), Url::parse(&url)?.host_str(), item.title()) {
            (Some(u), Some(s), Some(t)) => (u, s.to_string(), t),
            _ => continue,
        };

        if urls.contains(url) {
            println!("Skipping already seen article: {} [{}]", title, url);
            continue;
        }
        urls.insert(url.to_string());

        println!("Processing article: {} [{}]", title, url);

        let article = Article::new(url.to_string(), title.to_string());
        let article_words = process_article(&article)?;
        index.add(
            site.to_string(),
            title.to_string(),
            url.to_string(),
            article_words,
        );
    }
    Result::Ok(())
}
