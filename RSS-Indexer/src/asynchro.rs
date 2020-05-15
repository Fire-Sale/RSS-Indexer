use futures::stream::{self, StreamExt};
use rss::Channel;
use scraper::{Html, Selector};
use std::fs::File;
use std::io::BufReader;
use url::Url;

use crate::common::*;

const MAX_CONN: usize = 59;  // 59

/// Open the file and use the `rss` crate to read the file and get a list of `Item`s (namely,
/// feeds). For each feed in the list, get the URL and call `process_feed`. Take a look at the
/// examples in the `rss` crate.
pub async fn process_feed_file(file_name: &str) -> RssIndexResult<ArticleIndex> {

    let mut index = ArticleIndex::new();

    let file = File::open(file_name)?;
    println!("Processing feed file: {}", file_name);

    let channel = Channel::read_from(BufReader::new(file))?;
    let mut tasks = Vec::new();

    for feed in channel.into_items() {
        let url = feed.link().ok_or(RssIndexError::UrlError)?.to_string();
        let title = feed.title().ok_or(RssIndexError::UrlError)?.to_string();

        let title = title.to_string();
        let url = url.to_string();

        tasks.push(async move {
            println!("Processing feed: {} [{}]", title, url);
            process_feed(&url).await
        });
    }

    let all_results_stream = stream::iter(tasks);
    let all_results = all_results_stream.buffer_unordered(MAX_CONN)
    .filter_map(|res| async move {
        if let Ok(result) = res {
            Some(result)
        } else {
            None
        }
    })
    .collect::<Vec<_>>().await;

    for (site, title, url, article_words) in all_results.into_iter().flatten() {
        index.add(site, title, url, article_words);
    }

    RssIndexResult::Ok(index)
}



/// Read a feed file from a URL, build an rss channel from it, and iterate through the `Item`s
/// (articles). Pull out three pieces of information: the URL, the hostname, and the title (see
/// `Item::link`, `Url::parse`, and `Url::host_str` here). Process each url/title with
/// `process_article`, and then add it to the input `ArticleIndex` along with the hostname. If an
/// `Item` is missing a url/hostname/title, skip it (do not panic).
async fn process_feed(url: &str) -> RssIndexResult<Vec<(String, String, String, Bag<String>)>> {
    
    let mut tasks = Vec::new();

    let contents = reqwest::get(url).await?.bytes().await?;
    let channel = Channel::read_from(&contents[..])?;
    let items = channel.into_items();

    for item in items {
        let (url, site, title) = match (
            item.link(),
            Url::parse(item.link().unwrap())?.host_str(),
            item.title(),
        ) {
            (Some(u), Some(s), Some(t)) => (u, s.to_string(), t),
            _ => continue,
        };

        let url = url.to_string();
        let title = title.to_string();
        let site = site.to_string();

        tasks.push(async move {
            println!("Processing article: {} [{}]", title, url);
            let words = process_article(&url).await?;
            RssIndexResult::Ok((site, title, url, words))
        });
    }

    let all_results_stream = stream::iter(tasks);
    let all_results = all_results_stream.buffer_unordered(MAX_CONN)
    .filter_map(|res| async move {
        if let Ok(result) = res {
            Some(result)
        } else {
            None
        }
    })
    .collect::<Vec<_>>().await;

    RssIndexResult::Ok(all_results)
}

/// Delimiters for splitting a string
const DELIMS: &str = " \t\r\n!@#$%^&*()_-+=~`{[}]|\\\"':;<,>.?/";

/// Use `reqwest` to fetch the article URL, use `scraper` to parse the document, select the "body"
/// tag, and get the text, split each string on `DELIMS` (see `std::string::split` and
/// `std::string::contains`), convert each piece to lowercase, and return a vector of words
/// appearing in the article (possibly with duplicates).
async fn process_article(url: &str) -> RssIndexResult<Bag<String>> {
    
    let mut words = Bag::new();
    let contents = reqwest::get(url).await?.text().await?;
    let parsed = Html::parse_document(&contents);
    let body_selector = Selector::parse("body").map_err(|_| RssIndexError::ScraperError)?;
    let body = parsed
        .select(&body_selector)
        .next()
        .ok_or(RssIndexError::ScraperError)?;

    for text in body.text() {
        for tok in text.split(|c| DELIMS.contains(c)) {
            if !tok.is_empty() {
                words.add(tok.to_string().to_lowercase());
            }
        }
    }

    RssIndexResult::Ok(words)
}
